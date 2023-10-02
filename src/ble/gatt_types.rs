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

use bytemuck_derive::{Pod, Zeroable};
use defmt::Format;
use nrf_softdevice::ble::GattValue;

#[derive(Copy, Clone)]
pub(crate) enum DataOpcode {
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
pub(crate) struct DataPoint {
    opcode: u8,
    length: u8,
    value: [u8; 8],
}

impl DataPoint {
    /// Create a new `DataPoint` from scratch
    ///
    /// One should prefer creating a `DataPoint` from a `DataOpcode` to ensure that the packet is
    /// correctly formed.
    pub(crate) fn from_parts(opcode: u8, length: u8, value: [u8; 8]) -> Self {
        DataPoint {
            opcode,
            length,
            value,
        }
    }
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
    /// Minimum = one opcode byte and one length byte
    const MIN_SIZE: usize = 2;
    const MAX_SIZE: usize = core::mem::size_of::<Self>();

    fn from_gatt(_data: &[u8]) -> Self {
        unimplemented!("DataPoint is only used for outgoing data");
    }

    fn to_gatt(&self) -> &[u8] {
        let length = self.length + 2;
        &bytemuck::bytes_of(self)[..length.into()]
    }
}

#[derive(Copy, Clone)]
pub(crate) enum ControlOpcode {
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
    Unknown(u8),
    Invalid,
}

impl ControlOpcode {
    pub(crate) fn is_known_opcode(&self) -> bool {
        !matches!(self, Self::Unknown(_) | Self::Invalid)
    }
}

impl Format for ControlOpcode {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            ControlOpcode::Tare => defmt::write!(fmt, "Tare"),
            ControlOpcode::StartMeasurement => defmt::write!(fmt, "StartMeasurement"),
            ControlOpcode::StopMeasurement => defmt::write!(fmt, "StopMeasurement"),
            ControlOpcode::StartPeakRfdMeasurement => defmt::write!(fmt, "StartPeakRfdMeasurement"),
            ControlOpcode::StartPeakRfdMeasurementSeries => {
                defmt::write!(fmt, "StartPeakRfdMeasurementSeries");
            }
            ControlOpcode::AddCalibrationPoint(val) => {
                defmt::write!(fmt, "AddCalibrationPoint {=f32}", val);
            }
            ControlOpcode::SaveCalibration => defmt::write!(fmt, "SaveCalibration"),
            ControlOpcode::GetAppVersion => defmt::write!(fmt, "GetAppVersion"),
            ControlOpcode::GetErrorInfo => defmt::write!(fmt, "GetErrorInfo"),
            ControlOpcode::ClearErrorInfo => defmt::write!(fmt, "ClearErrorInfo"),
            ControlOpcode::Shutdown => defmt::write!(fmt, "Shutdown"),
            ControlOpcode::SampleBattery => defmt::write!(fmt, "SampleBattery"),
            ControlOpcode::GetProgressorID => defmt::write!(fmt, "GetProgressorID"),
            ControlOpcode::Unknown(opcode) => defmt::write!(fmt, "Unknown (0x{=u8:X})", opcode),
            ControlOpcode::Invalid => defmt::write!(fmt, "Invalid"),
        }
    }
}

impl GattValue for ControlOpcode {
    const MIN_SIZE: usize = 1;
    const MAX_SIZE: usize = 6;

    fn from_gatt(data: &[u8]) -> Self {
        if data.len() < Self::MIN_SIZE || data.len() > Self::MAX_SIZE {
            defmt::error!("Control payload size out of range: {=usize}", data.len());
            return Self::Invalid;
        }
        let opcode = data[0];
        match opcode {
            0x64 => Self::Tare,
            0x65 => Self::StartMeasurement,
            0x66 => Self::StopMeasurement,
            0x67 => Self::StartPeakRfdMeasurement,
            0x68 => Self::StartPeakRfdMeasurementSeries,
            0x69 => {
                // Allow length to be omitted
                let float_bytes = match data.len() {
                    5 => &data[1..5],
                    6 => &data[2..6],
                    _ => {
                        defmt::error!("Invalid payload {=[u8]:X}", data);
                        return Self::Invalid;
                    }
                };
                Self::AddCalibrationPoint(f32::from_le_bytes(float_bytes.try_into().unwrap()))
            }
            0x6A => Self::SaveCalibration,
            0x6B => Self::GetAppVersion,
            0x6C => Self::GetErrorInfo,
            0x6D => Self::ClearErrorInfo,
            0x6E => Self::Shutdown,
            0x6F => Self::SampleBattery,
            0x70 => Self::GetProgressorID,
            _ => Self::Unknown(opcode),
        }
    }

    fn to_gatt(&self) -> &[u8] {
        unimplemented!("ControlOpcode is only used for incoming messages")
    }
}
