// Copyright 2023 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Janky driver to store values in non-volatile memory e.g. calibration constants
//!
//! Data is stored in a pre-determined location in Flash and cached in RAM. A checksum for the
//! entire block is stored in Flash, and if there is a mismatch, everything is replaed with default
//! values.
//!
//! Ideally, we'd use the nrf52's UICR registers, which are made for this purpose, but it's
//! impossible to write to them with the Softdevice enabled. Instead, we just reserve one 4kB page
//! of Flash.

use aligned::{Aligned, A32};
use as_slice::AsMutSlice;
use bytemuck_derive::{Pod, Zeroable};
use crc::{Crc, CRC_32_ISCSI};
use embedded_storage::nor_flash::ReadNorFlash;
use embedded_storage_async::nor_flash::NorFlash;
use nrf_softdevice::{Flash, Softdevice};

/// Address of start of Flash page
#[cfg(feature = "nrf52840")]
const MIN_ADDR: u32 = 0xDF000;
#[cfg(feature = "nrf52832")]
const MIN_ADDR: u32 = 0x3F000;
/// Address of start of next Flash page
#[cfg(feature = "nrf52840")]
const MAX_ADDR: u32 = 0xE0000;
#[cfg(feature = "nrf52832")]
const MAX_ADDR: u32 = 0x40000;
const CHECKSUM_ADDR: u32 = MAX_ADDR - 4;

/// Data stored in Flash
///
/// The struct is stored as is, but aligned via `AlignedCache`. Care must be taken if new fields
/// are added, as the checksum mechanism will cause all stored values to be reset without migration
/// code.
#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C, packed)]
struct Cache {
    calibration_m: f32,
    calibration_b: i32,
}
// Ensure that we only read into and write from 4-byte aligned buffers
type AlignedCache = Aligned<A32, Cache>;

impl Default for Cache {
    fn default() -> Self {
        Self {
            calibration_m: crate::weight::DEFAULT_CALIBRATION_M,
            calibration_b: crate::weight::DEFAULT_CALIBRATION_B,
        }
    }
}

fn checksum(bytes: &[u8]) -> [u8; 4] {
    let crc = Crc::<u32>::new(&CRC_32_ISCSI);
    crc.checksum(bytes).to_le_bytes()
}

pub struct Nvm {
    flash: Flash,
    cache: AlignedCache,
    dirty: bool,
}

impl Nvm {
    pub fn new(sd: &Softdevice) -> Self {
        let flash = Flash::take(sd);
        let mut new = Self {
            flash,
            cache: Aligned::default(),
            dirty: false,
        };
        new.flash
            .read(MIN_ADDR, bytemuck::bytes_of_mut(&mut *new.cache))
            .unwrap();
        // Must only read into 4-byte aligned buffer
        let mut stored_checksum: Aligned<A32, [u8; 4]> = Aligned::default();
        new.flash
            .read(CHECKSUM_ADDR, stored_checksum.as_mut_slice())
            .unwrap();
        let load_defaults = *stored_checksum != checksum(bytemuck::bytes_of(&*new.cache));

        if load_defaults {
            defmt::info!("Checksum mismatch. Rewriting NVM defaults.");
            new.cache = AlignedCache::default();
        }
        new
    }

    pub fn write_cal_m(&mut self, val: f32) {
        self.cache.calibration_m = val;
        self.dirty = true;
    }

    pub fn read_cal_m(&self) -> f32 {
        self.cache.calibration_m
    }

    pub fn write_cal_b(&mut self, val: i32) {
        self.cache.calibration_b = val;
        self.dirty = true;
    }

    pub fn read_cal_b(&self) -> i32 {
        self.cache.calibration_b
    }

    pub async fn flush(&mut self) {
        if !self.dirty {
            return;
        }
        let raw_cache = bytemuck::bytes_of(&*self.cache);
        let mut aligned_checksum: Aligned<A32, [u8; 4]> = Aligned::default();
        *aligned_checksum = checksum(raw_cache);
        self.flash
            .erase(MIN_ADDR, MAX_ADDR)
            .await
            .expect("Erase to succeed");
        self.flash
            .write(MIN_ADDR, raw_cache)
            .await
            .expect("Write to succeed");
        self.flash
            .write(CHECKSUM_ADDR, &*aligned_checksum)
            .await
            .expect("Write to succeed");
    }
}
