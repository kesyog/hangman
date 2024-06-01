// Copyright 2024 Google LLC
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

use defmt::Format;
use num_traits::PrimInt;

#[derive(Copy, Clone, Format)]
pub struct CalPoint<Reading> {
    pub expected_value: f32,
    pub measured_value: Reading,
}

#[derive(Copy, Clone, Default, Format)]
pub struct TwoPoint<Reading> {
    zero: Option<Reading>,
    other: Option<CalPoint<Reading>>,
}

/// Calibration constants
///
/// weight = m * (reading - b)
#[derive(Copy, Clone, Format)]
pub struct Constants<Reading> {
    pub m: f32,
    pub b: Reading,
}

impl<Reading: PrimInt + Format> TwoPoint<Reading> {
    pub fn add_point(&mut self, point: CalPoint<Reading>) {
        crate::debug!("New calibration point: {}", point);
        if point.expected_value == 0.0 {
            self.zero = Some(point.measured_value);
        } else {
            self.other = Some(point);
        }
    }

    pub fn get_cal_constants(&self) -> Option<Constants<Reading>> {
        let other = self.other?;
        let b = self.zero?;
        if other.measured_value == b {
            crate::error!("Attempted divide by zero");
            return None;
        }
        let Some(denominator) = other.measured_value.checked_sub(&b) else {
            crate::error!("Underflow: {} - {}", other.measured_value, b);
            return None;
        };
        let m = other.expected_value / denominator.to_f32().unwrap();
        Some(Constants { m, b })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn calibrate() {
        let mut cal = TwoPoint::default();
        let zero = CalPoint {
            expected_value: 0.0,
            measured_value: 0x1234,
        };
        let other = CalPoint {
            expected_value: 100.0,
            measured_value: 0x4567,
        };
        cal.add_point(zero);
        cal.add_point(other);
        let Constants { m, b } = cal.get_cal_constants().unwrap();
        assert_eq!((zero.measured_value - b) as f32 * m, zero.expected_value);
        assert_eq!((other.measured_value - b) as f32 * m, other.expected_value);
    }
}
