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

mod advertising;
mod gatt_server;
mod gatt_types;
mod task;

use nrf_softdevice::Softdevice;
pub use task::task as task_fn;

use crate::{weight, MEASURE_COMMAND_CHANNEL_SIZE};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Sender;

type MeasureChannel = Sender<'static, NoopRawMutex, weight::Command, MEASURE_COMMAND_CHANNEL_SIZE>;

/// Device name to be used in GAP and advertising data. The Tindeq app requires this to be
/// something of this form.
const ADVERTISED_NAME: &[u8] = env!("ADVERTISED_NAME").as_bytes();

fn softdevice_config() -> nrf_softdevice::Config {
    use nrf_softdevice::raw;
    let advertised_name_len: u16 = ADVERTISED_NAME.len() as u16;
    nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_XTAL as u8,
            rc_ctiv: 0,
            rc_temp_ctiv: 0,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 1,
            event_length: raw::BLE_GAP_EVENT_LENGTH_DEFAULT as u16,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t {
            // Set to something small-ish since individual GATT transactions are small (guessing
            // ~10 bytes). Might want to bump this up if we add DFU support. Don't really know what
            // I'm doing here.
            att_mtu: 48,
        }),
        gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t {
            // Using default value of BLE_GATTS_ATTR_TAB_SIZE_DEFAULT
            attr_tab_size: 1408,
        }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: 1,
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            p_value: ADVERTISED_NAME.as_ptr().cast_mut(),
            current_len: advertised_name_len,
            max_len: advertised_name_len,
            write_perm: unsafe { core::mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(
                raw::BLE_GATTS_VLOC_STACK as u8,
            ),
        }),
        ..Default::default()
    }
}

/// Initialize the Softdevice.
///
/// To keep the Softdevice machinery happy, the returned Softdevice should be "run" (e.g. via
/// `run`, `run_with_callback`, etc.) on its own task and given a chance to run as early before
/// running any other initialization code.
pub fn init_softdevice() -> &'static mut Softdevice {
    let sd = Softdevice::enable(&softdevice_config());
    gatt_server::init(sd).unwrap();
    sd
}
