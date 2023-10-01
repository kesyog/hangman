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

use nrf_softdevice::ble::peripheral::AdvertiseError;
use nrf_softdevice::ble::{Connection, Phy, TxPower};
use nrf_softdevice::{ble, raw as raw_sd, Softdevice};

const ADVERTISING_TIMEOUT_SEC: u16 = 3 * 60;

#[rustfmt::skip]
const ADVERTISING_DATA: &[u8] = &[
    2,
    raw_sd::BLE_GAP_AD_TYPE_FLAGS as u8,
    (raw_sd::BLE_GAP_ADV_FLAG_LE_GENERAL_DISC_MODE | raw_sd::BLE_GAP_ADV_FLAG_BR_EDR_NOT_SUPPORTED) as u8,
    16,
    raw_sd::BLE_GAP_AD_TYPE_COMPLETE_LOCAL_NAME as u8,
    b'P', b'r', b'o', b'g', b'r', b'e', b's', b's', b'o', b'r', b'_', b'1', b'7', b'1', b'9',
];

#[rustfmt::skip]
const SCAN_RESPONSE_DATA: &[u8] = &[
    17,
    raw_sd::BLE_GAP_AD_TYPE_128BIT_SERVICE_UUID_COMPLETE as u8,
    0x57, 0xad, 0xfe, 0x4f, 0xd3, 0x13, 0xcc, 0x9d, 0xc9, 0x40, 0xa6, 0x1e, 0x01, 0x17, 0x4e, 0x7e,
];

pub(crate) async fn start(sd: &Softdevice) -> Result<Connection, AdvertiseError> {
    let config = ble::peripheral::Config {
        // Timeout is passed as # of 10 ms periods
        timeout: Some(ADVERTISING_TIMEOUT_SEC * (1000 / 10)),
        // Primary PHY must be 1M
        primary_phy: Phy::M1,
        secondary_phy: Phy::M2,
        // Empirically, -40dB definitely does not work and -4dB seems to work
        // There's probably some power savings to be gained here by lowering this further, but the
        // Nordic guidance suggests diminishing returns and I'd rather err on the side of
        // rock-solid connectivity.
        // From Nordic: "At 0dBm with the DC/DC on, the nRF52832 transmitter draws 5.3mA.
        // Increasing the TX power to +4dBm adds only 2.2mA. Decreasing it to -40 dBm saves only
        // 2.6mA."
        tx_power: TxPower::Minus4dBm,
        ..Default::default()
    };
    let adv = ble::peripheral::ConnectableAdvertisement::ScannableUndirected {
        adv_data: ADVERTISING_DATA,
        scan_data: SCAN_RESPONSE_DATA,
    };
    ble::peripheral::advertise_connectable(sd, adv, &config).await
}
