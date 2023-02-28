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

/// Janky driver to store stuff in non-volatile memory e.g. calibration constants
///
/// Ideally, we'd use the nrf52's UICR registers, which are made for this purpose, but it's
/// impossible to write to them with the Softdevice enabled. Instead, we just reserve one 4kB page
/// of Flash.
///
/// TODO: consider alternate flow
/// 1. Write new values to uninit RAM
/// 2. Reboot
/// 3. Write to UICR before initializing Softdevice
use crc::{Crc, CRC_32_ISCSI};
use embedded_storage::nor_flash::ReadNorFlash;
use embedded_storage_async::nor_flash::AsyncNorFlash;
use nrf_softdevice::{Flash, Softdevice};
use strum::{EnumCount as _, IntoEnumIterator as _};
use strum_macros::{EnumCount, EnumDiscriminants, EnumIter};

/// Address of start of Flash page
const MIN_ADDR: u32 = 0xDF000;
/// Address of start of next Flash page
const MAX_ADDR: u32 = 0xE0000;
const CHECKSUM_ADDR: u32 = MAX_ADDR - 4;

#[derive(EnumDiscriminants, Clone, Copy)]
#[strum_discriminants(name(RegisterRead), derive(EnumCount, EnumIter))]
pub enum RegisterWrite {
    CalibrationM(f32),
    CalibrationB(i32),
}

impl RegisterWrite {
    /// Index within cache array
    fn address(&self) -> usize {
        RegisterRead::from(self).address()
    }

    fn to_bytes(self) -> [u8; 4] {
        match self {
            RegisterWrite::CalibrationM(val) => val.to_le_bytes(),
            RegisterWrite::CalibrationB(val) => val.to_le_bytes(),
        }
    }
}

impl RegisterRead {
    /// Index within cache array
    const fn address(&self) -> usize {
        *self as usize
    }

    const fn default(&self) -> RegisterWrite {
        match self {
            RegisterRead::CalibrationM => {
                RegisterWrite::CalibrationM(crate::weight::DEFAULT_CALIBRATION_M)
            }
            RegisterRead::CalibrationB => {
                RegisterWrite::CalibrationB(crate::weight::DEFAULT_CALIBRATION_B)
            }
        }
    }
}

fn checksum(bytes: &[u8]) -> [u8; 4] {
    let crc = Crc::<u32>::new(&CRC_32_ISCSI);
    crc.checksum(bytes).to_le_bytes()
}

pub struct Nvm {
    flash: Flash,
    cache: [[u8; 4]; RegisterRead::COUNT],
    dirty: bool,
}

impl Nvm {
    pub fn new(sd: &Softdevice) -> Self {
        let mut flash = Flash::take(sd);
        let mut cache: [u8; 4 * RegisterRead::COUNT] = Default::default();
        flash.read(MIN_ADDR, &mut cache).unwrap();
        let mut stored_checksum: [u8; 4] = Default::default();
        flash.read(CHECKSUM_ADDR, &mut stored_checksum).unwrap();
        let load_defaults = stored_checksum != checksum(&cache);

        let mut new = Self {
            flash,
            cache: bytemuck::cast(cache),
            dirty: false,
        };
        if load_defaults {
            defmt::info!("Checksum mismatch. Rewriting NVM defaults.");
            for reg in RegisterRead::iter() {
                new.write(reg.default());
            }
        }
        new
    }

    pub fn write(&mut self, reg: RegisterWrite) {
        self.cache[reg.address()] = reg.to_bytes();
        self.dirty = true;
    }

    pub fn read(&self, reg: RegisterRead) -> [u8; 4] {
        self.cache[reg.address()]
    }

    pub async fn flush(&mut self) {
        if !self.dirty {
            return;
        }
        let raw_cache = bytemuck::cast_slice(&self.cache);
        let checksum = checksum(raw_cache);
        self.flash
            .erase(MIN_ADDR, MAX_ADDR)
            .await
            .expect("Erase to succeed");
        self.flash
            .write(MIN_ADDR, raw_cache)
            .await
            .expect("Write to succeed");
        self.flash
            .write(CHECKSUM_ADDR, &checksum)
            .await
            .expect("Write to succeed");
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn addresses() {
        // Ensure that all of the registers and 4-byte checksum can fit on our Flash page
        assert!(4 * (RegisterRead::COUNT + 1) <= MAX_ADDR - MIN_ADDR);
    }
}
