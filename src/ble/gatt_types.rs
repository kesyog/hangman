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

#[derive(Copy, Clone, Format)]
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
    /// One should prefer creating a `DataPoint` from a `DataOpcode` to ensure that all invariants
    /// are maintained.
    pub(crate) unsafe fn from_parts(opcode: u8, length: u8, value: [u8; 8]) -> Self {
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
}

impl ControlOpcode {
    pub(crate) const fn opcode(self) -> u8 {
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
pub(crate) struct ControlPoint {
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
