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

use crate::weight::Command as WeightCommand;
use crate::MEASURE_COMMAND_CHANNEL_SIZE;
use alloc::rc::Rc;
use defmt::Format;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Sender;
use nrf_softdevice::ble::gatt_server::NotifyValueError;
use nrf_softdevice::ble::peripheral::AdvertiseError;
use nrf_softdevice::ble::{gatt_server, Connection, GattValue};
use nrf_softdevice::{ble, raw as raw_sd, Softdevice};
use zerocopy::{AsBytes, FromBytes};

type MeasureChannel = Sender<'static, NoopRawMutex, WeightCommand, MEASURE_COMMAND_CHANNEL_SIZE>;

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

pub mod server {
    use super::Server;
    use nrf_softdevice::Softdevice;
    use once_cell::sync::OnceCell;

    static GATT_SERVER: OnceCell<Server> = OnceCell::new();

    pub fn init(sd: &mut Softdevice) -> Result<(), ()> {
        GATT_SERVER.set(Server::new(sd).unwrap()).map_err(|_| ())
    }

    pub(super) fn get() -> &'static Server {
        GATT_SERVER.get().expect("GATT_SERVER to be initialized")
    }
}

#[derive(Copy, Clone, Format)]
pub enum DataOpcode {
    BatteryVoltage(u32),
    Weight(f32, u32),
    LowPowerWarning,
}

impl DataOpcode {
    fn opcode(&self) -> u8 {
        match self {
            DataOpcode::BatteryVoltage(..) => 0x00,
            DataOpcode::Weight(..) => 0x01,
            DataOpcode::LowPowerWarning => 0x04,
        }
    }

    fn length(&self) -> u8 {
        match self {
            DataOpcode::BatteryVoltage(..) => 4,
            DataOpcode::Weight(..) => 8,
            DataOpcode::LowPowerWarning => 0,
        }
    }

    fn value(&self) -> [u8; 8] {
        let mut value = [0; 8];
        match self {
            DataOpcode::BatteryVoltage(voltage) => {
                value[0..4].copy_from_slice(&voltage.to_le_bytes())
            }
            DataOpcode::Weight(weight, timestamp) => {
                value[0..4].copy_from_slice(&weight.to_le_bytes());
                value[4..].copy_from_slice(&timestamp.to_le_bytes());
            }
            DataOpcode::LowPowerWarning => (),
        };
        value
    }
}

#[derive(AsBytes)]
#[repr(C, packed)]
pub struct DataPoint {
    opcode: u8,
    length: u8,
    value: [u8; 8],
}

impl From<DataOpcode> for DataPoint {
    fn from(opcode: DataOpcode) -> Self {
        Self {
            opcode: opcode.opcode(),
            length: opcode.length(),
            value: opcode.value(),
        }
    }
}

impl GattValue for DataPoint {
    const MIN_SIZE: usize = 2;
    const MAX_SIZE: usize = 10;

    fn from_gatt(data: &[u8]) -> Self {
        if data.len() < 2 {
            panic!("DataPoint is too small");
        }
        let mut value = [0; 8];
        let length = usize::min(data.len() - 2, data[1] as usize).min(value.len());
        value[0..length].copy_from_slice(&data[2..2 + length]);
        Self {
            opcode: data[0],
            length: length as u8,
            value,
        }
    }

    fn to_gatt(&self) -> &[u8] {
        let length = self.length + 2;
        &self.as_bytes()[..length.into()]
    }
}

#[derive(Copy, Clone, Format)]
pub enum ControlOpcode {
    Tare = 0x64,
    StartMeasurement = 0x65,
    StopMeasurement = 0x66,
    StartPeakRfdMeasurement = 0x67,
    StartPeakRfdMeasurementSeries = 0x68,
    AddCalibrationPoint = 0x69,
    SaveCalibration = 0x6A,
    GetAppVersion = 0x6B,
    GetErrorInfo = 0x6C,
    ClearErrorInfo = 0x6D,
    Shutdown = 0x6E,
    SampleBattery = 0x6F,
}

#[derive(FromBytes, AsBytes)]
#[repr(C, packed)]
pub struct ControlPoint {
    opcode: u8,
    length: u8,
}

impl From<ControlOpcode> for ControlPoint {
    fn from(opcode: ControlOpcode) -> Self {
        Self {
            opcode: opcode as u8,
            length: 0,
        }
    }
}

impl TryFrom<ControlPoint> for ControlOpcode {
    type Error = u8;

    // TODO: can we derive this?
    fn try_from(value: ControlPoint) -> Result<Self, Self::Error> {
        match value.opcode {
            0x64 => Ok(ControlOpcode::Tare),
            0x65 => Ok(ControlOpcode::StartMeasurement),
            0x66 => Ok(ControlOpcode::StopMeasurement),
            0x67 => Ok(ControlOpcode::StartPeakRfdMeasurement),
            0x68 => Ok(ControlOpcode::StartPeakRfdMeasurementSeries),
            0x69 => Ok(ControlOpcode::AddCalibrationPoint),
            0x6A => Ok(ControlOpcode::SaveCalibration),
            0x6B => Ok(ControlOpcode::GetAppVersion),
            0x6C => Ok(ControlOpcode::GetErrorInfo),
            0x6D => Ok(ControlOpcode::ClearErrorInfo),
            0x6E => Ok(ControlOpcode::Shutdown),
            0x6F => Ok(ControlOpcode::SampleBattery),
            other => Err(other),
        }
    }
}

impl GattValue for ControlPoint {
    const MIN_SIZE: usize = 2;
    const MAX_SIZE: usize = 2;

    fn from_gatt(data: &[u8]) -> Self {
        Self::read_from(data).unwrap()
    }

    fn to_gatt(&self) -> &[u8] {
        let length = self.length + 2;
        &self.as_bytes()[..length.into()]
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
    control: ControlPoint,
}

#[nrf_softdevice::gatt_server]
struct Server {
    progressor: ProgressorService,
}

pub fn notify_data(data: DataOpcode, connection: &Connection) -> Result<(), NotifyValueError> {
    let raw_data = DataPoint::from(data);
    server::get().progressor.data_notify(connection, &raw_data)
}

// not really gatt. oops
async fn advertise(sd: &Softdevice) -> Result<Connection, AdvertiseError> {
    let config = ble::peripheral::Config::default();
    let adv = ble::peripheral::ConnectableAdvertisement::ScannableUndirected {
        adv_data: ADVERTISING_DATA,
        scan_data: SCAN_RESPONSE_DATA,
    };
    ble::peripheral::advertise_connectable(sd, adv, &config).await
}

#[embassy_executor::task]
pub async fn ble_task(sd: &'static Softdevice, measure_ch: MeasureChannel) {
    defmt::info!("Starting BLE task");
    let server = server::get();
    loop {
        crate::leds::singleton_get().lock().await.rgb_blue.set_low();
        let conn = advertise(sd).await.unwrap();
        defmt::info!("Peer connected");
        {
            let mut leds = crate::leds::singleton_get().lock().await;
            leds.rgb_blue.set_high();
            leds.green.set_low();
        }

        gatt_server::run(&conn, server, |e| match e {
            ServerEvent::Progressor(e) => match e {
                ProgressorServiceEvent::ControlWrite(val) => {
                    let control_op = ControlOpcode::try_from(val);
                    match control_op {
                        Ok(op) => defmt::info!("ProgressorService.ControlWrite: {}", op),
                        Err(op) => defmt::info!("ProgressorService.ControlWrite: 0x{:02X}", op),
                    }
                    match control_op {
                        Ok(ControlOpcode::Tare) => {
                            if measure_ch.try_send(WeightCommand::Tare).is_err() {
                                defmt::error!("Failed to send Tare");
                            }
                        }
                        Ok(ControlOpcode::StartMeasurement) => {
                            let notify_cb = Rc::new({
                                let conn = conn.clone();
                                move |timestamp: u32, measurement: f32| {
                                    if notify_data(
                                        DataOpcode::Weight(measurement, timestamp),
                                        &conn,
                                    )
                                    .is_err()
                                    {
                                        defmt::error!("Notify failed");
                                    }
                                }
                            });
                            if measure_ch
                                .try_send(WeightCommand::StartMeasurement(notify_cb))
                                .is_err()
                            {
                                defmt::error!("Failed to send StartMeasurement");
                            }
                        }
                        Ok(ControlOpcode::StopMeasurement) => {
                            if measure_ch.try_send(WeightCommand::StopMeasurement).is_err() {
                                defmt::error!("Failed to send StopMeasurement");
                            }
                        }
                        _ => (),
                    }
                }
                ProgressorServiceEvent::DataCccdWrite { notifications } => {
                    defmt::info!("DataCccdWrite: {}", notifications);
                }
            },
        })
        .await;
        crate::leds::singleton_get().lock().await.green.set_high();
        // Make sure we stop measuring on disconnect
        measure_ch.send(WeightCommand::StopMeasurement).await;

        defmt::info!("Disconnected");
    }
}
