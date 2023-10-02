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

use super::{advertising, MeasureChannel};
use crate::button::Button;
use crate::{battery_voltage, weight};
use embassy_time::{Duration, Timer};
use nrf_softdevice::ble::{peripheral::AdvertiseError, Connection};
use nrf_softdevice::Softdevice;

async fn system_off(measure_ch: MeasureChannel, wakeup_button: Button) -> ! {
    // We shouldn't be sampling at this point, but just in case, stop sampling here.
    // 1. We want the ADC to be powered down while we are asleep
    // 2. Our system_off routine _might_ not work properly if there's a pending gpio
    //    event
    if measure_ch.try_send(weight::Command::StopSampling).is_err() {
        defmt::error!("Failed to send StopSampling");
    }
    Timer::after(Duration::from_millis(1000)).await;
    // We won't return from this
    // SAFETY: there are no pending GPIO events
    unsafe { crate::sleep::system_off(wakeup_button).await }
}

#[embassy_executor::task]
pub async fn task(sd: &'static Softdevice, measure_ch: MeasureChannel, wakeup_button: Button) {
    defmt::debug!("Starting BLE task");
    // Check for low battery voltage at startup
    if battery_voltage::is_critically_low() {
        defmt::error!("🔋💀 Battery voltage critically low!");
        system_off(measure_ch, wakeup_button).await;
    }

    const ADVERTISED_NAME_STR: Result<&str, core::str::Utf8Error> =
        core::str::from_utf8(super::ADVERTISED_NAME);
    defmt::info!("Advertising as {=str}", ADVERTISED_NAME_STR.unwrap());
    let conn: Connection = match advertising::start(sd).await {
        Ok(conn) => conn,
        Err(AdvertiseError::Timeout) => {
            defmt::warn!("Advertising timeout");
            system_off(measure_ch, wakeup_button).await
        }
        Err(AdvertiseError::NoFreeConn) => {
            defmt::error!("No free connection");
            system_off(measure_ch, wakeup_button).await
        }
        Err(AdvertiseError::Raw(err)) => {
            defmt::error!("Advertising error: {=u32}", err as u32);
            system_off(measure_ch, wakeup_button).await
        }
    };
    defmt::info!("Peer connected");
    super::gatt_server::run(&conn, &measure_ch).await;
    defmt::info!("Disconnected");
    // Make sure we stop measuring on disconnect
    measure_ch.send(weight::Command::StopSampling).await;
    system_off(measure_ch, wakeup_button).await;
}
