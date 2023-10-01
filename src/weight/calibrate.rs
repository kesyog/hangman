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

use super::{RawReading, Sample, SampleProducerMut};

pub struct Calibrator<T> {
    sampler: T,
    m: f32,
    b: RawReading,
}

impl<T> Calibrator<T> {
    pub fn new(sampler: T, m: f32, b: RawReading) -> Self {
        Self { sampler, m, b }
    }

    pub fn set_calibration(&mut self, m: f32, b: RawReading) {
        self.m = m;
        self.b = b;
    }

    fn calibrate(&self, raw_value: RawReading) -> f32 {
        let value = (raw_value - self.b) as f32 * self.m;
        defmt::trace!("Calibrated = {}", value);
        value
    }
}

impl<T> SampleProducerMut for Calibrator<T>
where
    T: SampleProducerMut<Output = RawReading>,
{
    type Output = f32;

    async fn sample(&mut self) -> Sample<Self::Output> {
        let Sample {
            timestamp,
            value: raw_value,
        } = self.sampler.sample().await;
        Sample {
            timestamp,
            value: self.calibrate(raw_value),
        }
    }
}

impl<T> SampleProducerMut for &mut Calibrator<T>
where
    T: SampleProducerMut<Output = RawReading>,
{
    type Output = f32;

    async fn sample(&mut self) -> Sample<<&mut Calibrator<T> as SampleProducerMut>::Output> {
        let Sample {
            timestamp,
            value: raw_value,
        } = self.sampler.sample().await;
        Sample {
            timestamp,
            value: self.calibrate(raw_value),
        }
    }
}
