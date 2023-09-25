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

use crate::button::Button;
use crate::{weight, MEASURE_COMMAND_CHANNEL_SIZE};
use alloc::boxed::Box;
use bytemuck_derive::{Pod, Zeroable};
use defmt::Format;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Sender;
use embassy_time::{Duration, Timer};
use nrf_softdevice::ble::gatt_server::NotifyValueError;
use nrf_softdevice::ble::peripheral::AdvertiseError;
use nrf_softdevice::ble::{gatt_server, Connection, GattValue, Phy, TxPower};
use nrf_softdevice::{ble, raw as raw_sd, Softdevice};

type MeasureChannel = Sender<'static, NoopRawMutex, weight::Command, MEASURE_COMMAND_CHANNEL_SIZE>;

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

// TODO: make this the source of truth for ADVERTISING_DATA
const ADVERTISED_NAME: &[u8] = b"Progressor_1719";
const DUMMY_VERSION_NUMBER: &[u8] = b"1.2.3.4";
const DUMMY_ID: u32 = 42;

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
    AppVersion(&'static [u8]),
    ProgressorId(u32),
}

impl DataOpcode {
    fn opcode(&self) -> u8 {
        match self {
            DataOpcode::BatteryVoltage(..)
            | DataOpcode::AppVersion(..)
            | DataOpcode::ProgressorId(..) => 0x00,
            DataOpcode::Weight(..) => 0x01,
            DataOpcode::LowPowerWarning => 0x04,
        }
    }

    fn length(&self) -> u8 {
        match self {
            DataOpcode::BatteryVoltage(..) | DataOpcode::ProgressorId(..) => 4,
            DataOpcode::Weight(..) => 8,
            DataOpcode::LowPowerWarning => 0,
            DataOpcode::AppVersion(version) => version.len() as u8,
        }
    }

    fn value(&self) -> [u8; 8] {
        let mut value = [0; 8];
        match self {
            DataOpcode::BatteryVoltage(voltage) => {
                value[0..4].copy_from_slice(&voltage.to_le_bytes());
            }
            DataOpcode::Weight(weight, timestamp) => {
                value[0..4].copy_from_slice(&weight.to_le_bytes());
                value[4..].copy_from_slice(&timestamp.to_le_bytes());
            }
            DataOpcode::LowPowerWarning => (),
            DataOpcode::ProgressorId(id) => {
                value[0..4].copy_from_slice(&id.to_le_bytes());
            }
            DataOpcode::AppVersion(version) => {
                value[0..version.len()].copy_from_slice(version);
            }
        };
        value
    }
}

#[derive(Copy, Clone, Pod, Zeroable)]
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
        assert!(data.len() >= 2, "DataPoint is too small");
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
        &bytemuck::bytes_of(self)[..length.into()]
    }
}

#[derive(Copy, Clone, Format)]
pub enum ControlOpcode {
    Tare,
    StartMeasurement,
    StopMeasurement,
    StartPeakRfdMeasurement,
    StartPeakRfdMeasurementSeries,
    AddCalibrationPoint(f32),
    SaveCalibration,
    GetAppVersion,
    GetErrorInfo,
    ClearErrorInfo,
    Shutdown,
    SampleBattery,
    GetProgressorID,
}

impl ControlOpcode {
    pub const fn opcode(self) -> u8 {
        match self {
            ControlOpcode::Tare => 0x64,
            ControlOpcode::StartMeasurement => 0x65,
            ControlOpcode::StopMeasurement => 0x66,
            ControlOpcode::StartPeakRfdMeasurement => 0x67,
            ControlOpcode::StartPeakRfdMeasurementSeries => 0x68,
            ControlOpcode::AddCalibrationPoint(_) => 0x69,
            ControlOpcode::SaveCalibration => 0x6A,
            ControlOpcode::GetAppVersion => 0x6B,
            ControlOpcode::GetErrorInfo => 0x6C,
            ControlOpcode::ClearErrorInfo => 0x6D,
            ControlOpcode::Shutdown => 0x6E,
            ControlOpcode::SampleBattery => 0x6F,
            ControlOpcode::GetProgressorID => 0x70,
        }
    }
}

#[derive(Copy, Clone, Default, Pod, Zeroable)]
#[repr(C, packed)]
pub struct ControlPoint {
    opcode: u8,
    length: u8,
    value: [u8; 4],
}

impl From<ControlOpcode> for ControlPoint {
    fn from(opcode: ControlOpcode) -> Self {
        Self {
            opcode: opcode.opcode(),
            length: 0,
            value: [0; 4],
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
            0x69 => Ok(ControlOpcode::AddCalibrationPoint(f32::from_le_bytes(
                value.value,
            ))),
            0x6A => Ok(ControlOpcode::SaveCalibration),
            0x6B => Ok(ControlOpcode::GetAppVersion),
            0x6C => Ok(ControlOpcode::GetErrorInfo),
            0x6D => Ok(ControlOpcode::ClearErrorInfo),
            0x6E => Ok(ControlOpcode::Shutdown),
            0x6F => Ok(ControlOpcode::SampleBattery),
            0x70 => Ok(ControlOpcode::GetProgressorID),
            other => Err(other),
        }
    }
}

impl GattValue for ControlPoint {
    // The length field may be omitted if there is no payload
    const MIN_SIZE: usize = 1;
    const MAX_SIZE: usize = 6;

    fn from_gatt(data: &[u8]) -> Self {
        if data.len() < Self::MIN_SIZE || data.len() > Self::MAX_SIZE {
            defmt::error!(
                "Bad control point received: opcode: {:X} len: {}",
                data[0],
                data.len()
            );
            return ControlPoint::default();
        }
        let opcode = data[0];
        let length = *data.get(1).unwrap_or(&0);
        if length == 0 {
            return Self {
                opcode,
                ..Default::default()
            };
        }
        if length as usize != data.len() - 2 {
            defmt::error!(
                "Length mismatch. Length: {}. Payload size: {}",
                length,
                data.len() - 2
            );
            return Self::default();
        }

        if length > 4 {
            defmt::error!("Invalid length: {}", length);
            return ControlPoint {
                opcode,
                ..Default::default()
            };
        }

        let mut value = [0; 4];
        value[0..length as usize].copy_from_slice(&data[2..2 + length as usize]);
        Self {
            opcode,
            length,
            value,
        }
    }

    fn to_gatt(&self) -> &[u8] {
        let length = self.length + 2;
        &bytemuck::bytes_of(self)[..length.into()]
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

pub fn softdevice_config() -> nrf_softdevice::Config {
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

fn notify_data(data: DataOpcode, connection: &Connection) -> Result<(), NotifyValueError> {
    let raw_data = DataPoint::from(data);
    server::get().progressor.data_notify(connection, &raw_data)
}

// Test function for sending out raw notifications
#[allow(dead_code)]
fn raw_notify_data(
    opcode: u8,
    raw_payload: &[u8],
    connection: &Connection,
) -> Result<(), NotifyValueError> {
    assert!(raw_payload.len() <= 8);
    let mut payload = [0; 8];
    payload[0..raw_payload.len()].copy_from_slice(raw_payload);

    let data = DataPoint {
        opcode,
        length: raw_payload.len().try_into().unwrap(),
        value: payload,
    };
    server::get().progressor.data_notify(connection, &data)
}

// not really gatt. oops
async fn advertise(sd: &Softdevice) -> Result<Connection, AdvertiseError> {
    let config = ble::peripheral::Config {
        // Timeout is passed as # of 10 ms periods
        timeout: Some(ADVERTISING_TIMEOUT_SEC * (1000 / 10)),
        // Primary PHY must be 1M
        primary_phy: Phy::M1,
        secondary_phy: Phy::M2,
        // TODO: how low can we reduce transmit power and get reasonable performance
        tx_power: TxPower::ZerodBm,
        ..Default::default()
    };
    let adv = ble::peripheral::ConnectableAdvertisement::ScannableUndirected {
        adv_data: ADVERTISING_DATA,
        scan_data: SCAN_RESPONSE_DATA,
    };
    ble::peripheral::advertise_connectable(sd, adv, &config).await
}

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
pub async fn ble_task(sd: &'static Softdevice, measure_ch: MeasureChannel, wakeup_button: Button) {
    defmt::debug!("Starting BLE task");
    let server = server::get();

    // crate::leds::singleton_get().lock().await.rgb_blue.set_low();
    const ADVERTISED_NAME_STR: Result<&str, core::str::Utf8Error> =
        core::str::from_utf8(ADVERTISED_NAME);
    defmt::info!("Advertising as {}", ADVERTISED_NAME_STR.unwrap());
    let conn: Connection = match advertise(sd).await {
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
            defmt::error!("Advertising error: {}", err as u32);
            system_off(measure_ch, wakeup_button).await
        }
    };
    defmt::info!("Peer connected");
    {
        /*
        let mut leds = crate::leds::singleton_get().lock().await;
        leds.rgb_blue.set_high();
        leds.green.set_low();
        */
    }

    gatt_server::run(&conn, server, |e| match e {
        ServerEvent::Progressor(e) => match e {
            ProgressorServiceEvent::ControlWrite(val) => {
                let control_op = ControlOpcode::try_from(val);
                match control_op {
                    Ok(op) => defmt::info!("ProgressorService.ControlWrite: {}", op),
                    Err(op) => defmt::warn!("ProgressorService.ControlWrite: 0x{:02X}", op),
                }
                match control_op {
                    Ok(ControlOpcode::Tare) => {
                        if measure_ch.try_send(weight::Command::Tare).is_err() {
                            defmt::error!("Failed to send Tare");
                        }
                    }
                    Ok(ControlOpcode::StartMeasurement) => {
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
                    Ok(ControlOpcode::StopMeasurement) => {
                        if measure_ch.try_send(weight::Command::StopSampling).is_err() {
                            defmt::error!("Failed to send StopSampling");
                        }
                    }
                    Ok(ControlOpcode::SampleBattery) => {
                        // Fake a battery voltage measurement
                        // TODO: add a real battery voltage measurement
                        if notify_data(DataOpcode::BatteryVoltage(3000), &conn).is_err() {
                            defmt::error!("Battery voltage response failed to send");
                        }
                    }
                    Ok(ControlOpcode::GetAppVersion) => {
                        if notify_data(DataOpcode::AppVersion(DUMMY_VERSION_NUMBER), &conn).is_err()
                        {
                            defmt::error!("Response to GetAppVersion failed");
                        };
                    }
                    Ok(ControlOpcode::GetProgressorID) => {
                        if notify_data(DataOpcode::ProgressorId(DUMMY_ID), &conn).is_err() {
                            defmt::error!("Response to GetProgressorID failed");
                        };
                    }
                    Ok(ControlOpcode::Shutdown) => {
                        // no-op. The peer should disconnect, which sends us to system oFF.
                    }
                    Ok(ControlOpcode::AddCalibrationPoint(known_weight)) => {
                        if measure_ch
                            .try_send(weight::Command::AddCalibrationPoint(known_weight))
                            .is_err()
                        {
                            defmt::error!("Failed to send AddCalibrationPoint");
                        }
                    }
                    Ok(ControlOpcode::SaveCalibration) => {
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
            ProgressorServiceEvent::DataCccdWrite { notifications } => {
                defmt::info!("DataCccdWrite: {}", notifications);
            }
        },
    })
    .await;
    // crate::leds::singleton_get().lock().await.green.set_high();
    // Make sure we stop measuring on disconnect
    measure_ch.send(weight::Command::StopSampling).await;

    defmt::info!("Disconnected");
    system_off(measure_ch, wakeup_button).await
}
