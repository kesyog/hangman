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

use super::RawReading;
use defmt::Format;

#[derive(Copy, Clone, Format)]
pub(crate) struct CalPoint {
    pub(crate) weight: f32,
    pub(crate) reading: RawReading,
}

#[derive(Copy, Clone, Default, Format)]
pub(crate) struct TwoPoint {
    zero: Option<RawReading>,
    other: Option<CalPoint>,
}

/// Calibration constants
///
/// weight = m * (reading - b)
#[derive(Copy, Clone, Format)]
pub(crate) struct Constants {
    pub(crate) m: f32,
    pub(crate) b: RawReading,
}

impl TwoPoint {
    pub(crate) fn add_point(&mut self, point: CalPoint) {
        defmt::debug!("New calibration point: {}", point);
        if point.weight == 0.0 {
            self.zero = Some(point.reading);
        } else {
            self.other = Some(point);
        }
    }

    pub(crate) fn get_cal_constants(&self) -> Option<Constants> {
        let other = self.other?;
        let b = self.zero?;
        if other.reading - b == 0 {
            defmt::warn!("Attempted divide by zero");
            return None;
        }
        let m = other.weight / (other.reading - b) as f32;
        Some(Constants { m, b })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn calibrate() {
        let mut cal = TwoPoint::default();
        let zero = CalPoint {
            weight: 0.0,
            reading: 0x1234,
        };
        let other = CalPoint {
            weight: 100.0,
            reading: 0x4567,
        };
        cal.add_point(zero);
        cal.add_point(other);
        let Constants { m, b } = cal.get_cal_constants();
        assert_eq!((zero.reading - b) as f32 * m, zero.weight);
        assert_eq!((other.reading - b) as f32 * m, other.weight);
    }
}
