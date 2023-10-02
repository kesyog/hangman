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

extern crate alloc;

use super::gatt_types::{ControlOpcode, DataOpcode, DataPoint};
use super::MeasureChannel;
use crate::{battery_voltage, weight};
use alloc::boxed::Box;
use embassy_time::Duration;
use nrf_softdevice::ble::gatt_server::NotifyValueError;
use nrf_softdevice::ble::Connection;
use nrf_softdevice::Softdevice;
use once_cell::sync::OnceCell;

const DUMMY_VERSION_NUMBER: &[u8] = b"1.2.3.4";
const DUMMY_ID: u32 = 42;

#[nrf_softdevice::gatt_server]
struct Server {
    progressor: ProgressorService,
}

impl Server {
    fn get() -> &'static Self {
        GATT_SERVER.get().expect("GATT_SERVER to be initialized")
    }
}

#[nrf_softdevice::gatt_service(uuid = "7e4e1701-1ea6-40c9-9dcc-13d34ffead57")]
struct ProgressorService {
    #[characteristic(uuid = "7e4e1702-1ea6-40c9-9dcc-13d34ffead57", notify)]
    data: DataPoint,

    #[characteristic(
        uuid = "7e4e1703-1ea6-40c9-9dcc-13d34ffead57",
        write,
        write_without_response
    )]
    control: ControlOpcode,
}

static GATT_SERVER: OnceCell<Server> = OnceCell::new();

pub(crate) fn init(sd: &mut Softdevice) -> Result<(), ()> {
    GATT_SERVER.set(Server::new(sd).unwrap()).map_err(|_| ())
}

fn notify_data(data: DataOpcode, connection: &Connection) -> Result<(), NotifyValueError> {
    Server::get()
        .progressor
        .data_notify(connection, &data.into())
}

/// Test function for sending out raw notifications
#[allow(dead_code)]
fn raw_notify_data(
    opcode: u8,
    raw_payload: &[u8],
    connection: &Connection,
) -> Result<(), NotifyValueError> {
    assert!(raw_payload.len() <= 8);
    let mut payload = [0; 8];
    payload[0..raw_payload.len()].copy_from_slice(raw_payload);

    let data = DataPoint::from_parts(opcode, raw_payload.len().try_into().unwrap(), payload);
    Server::get().progressor.data_notify(connection, &data)
}

fn on_control_message(message: ControlOpcode, conn: &Connection, measure_ch: &MeasureChannel) {
    if message.is_known_opcode() {
        defmt::info!("ProgressorService.ControlWrite: {}", message);
    } else {
        defmt::warn!("ProgressorService.ControlWrite: {}", message);
    }
    match message {
        ControlOpcode::Tare => {
            if measure_ch.try_send(weight::Command::Tare).is_err() {
                defmt::error!("Failed to send Tare");
            }
        }
        ControlOpcode::StartMeasurement => {
            let notify_cb = Box::new({
                let conn = conn.clone();
                move |duration_since_start: Duration, measurement: f32| {
                    if notify_data(
                        DataOpcode::Weight(
                            measurement,
                            u32::try_from(duration_since_start.as_micros()).unwrap(),
                        ),
                        &conn,
                    )
                    .is_err()
                    {
                        defmt::error!("Notify failed");
                    }
                }
            });
            if measure_ch
                .try_send(weight::Command::StartSampling(weight::SampleType::Tared(
                    Some(notify_cb),
                )))
                .is_err()
            {
                defmt::error!("Failed to send StartSampling");
            }
        }
        ControlOpcode::StopMeasurement => {
            if measure_ch.try_send(weight::Command::StopSampling).is_err() {
                defmt::error!("Failed to send StopSampling");
            }
        }
        ControlOpcode::SampleBattery => {
            let battery_voltage_mv =
                battery_voltage::get_startup_reading().expect("Battery to have been sampled");
            if notify_data(DataOpcode::BatteryVoltage(battery_voltage_mv), conn).is_err() {
                defmt::error!("Battery voltage response failed to send");
            }
        }
        ControlOpcode::GetAppVersion => {
            if notify_data(DataOpcode::AppVersion(DUMMY_VERSION_NUMBER), conn).is_err() {
                defmt::error!("Response to GetAppVersion failed");
            };
        }
        ControlOpcode::GetProgressorID => {
            if notify_data(DataOpcode::ProgressorId(DUMMY_ID), conn).is_err() {
                defmt::error!("Response to GetProgressorID failed");
            };
        }
        ControlOpcode::Shutdown => {
            // no-op. The peer should disconnect, which sends us to system oFF.
        }
        ControlOpcode::AddCalibrationPoint(known_weight) => {
            if measure_ch
                .try_send(weight::Command::AddCalibrationPoint(known_weight))
                .is_err()
            {
                defmt::error!("Failed to send AddCalibrationPoint");
            }
        }
        ControlOpcode::SaveCalibration => {
            if measure_ch
                .try_send(weight::Command::SaveCalibration)
                .is_err()
            {
                defmt::error!("Failed to send SaveCalibration");
            }
        }
        _ => (),
    }
}

/// Run gatt server until there is a disconnect
pub(crate) async fn run(conn: &Connection, measure_ch: &MeasureChannel) {
    let server = Server::get();

    nrf_softdevice::ble::gatt_server::run(conn, server, |e| match e {
        ServerEvent::Progressor(e) => match e {
            ProgressorServiceEvent::ControlWrite(value) => {
                if battery_voltage::is_low() {
                    defmt::error!("Low battery warning");
                    if notify_data(DataOpcode::LowPowerWarning, conn).is_err() {
                        defmt::error!("Failed to notify low power warning");
                    };
                    if conn.disconnect().is_err() {
                        defmt::error!("Failed to disconnect");
                    }
                }
                on_control_message(value, conn, measure_ch);
            }
            ProgressorServiceEvent::DataCccdWrite { notifications } => {
                defmt::info!("DataCccdWrite: {}", notifications);
            }
        },
    })
    .await;
}
