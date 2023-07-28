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

#![allow(unused)]

use super::{Sample, SampleProducerMut, SAMPLING_INTERVAL};
use core::num::NonZeroU32;
use embassy_time::{Duration, Instant, Timer};
use nrf_softdevice::Softdevice;
use once_cell::sync::Lazy;
use rand::RngCore;

struct SoftDeviceRng<'a>(&'a Softdevice);

impl<'a> RngCore for SoftDeviceRng<'a> {
    fn next_u32(&mut self) -> u32 {
        let mut buf = [0; 4];
        nrf_softdevice::random_bytes(self.0, &mut buf).unwrap();
        u32::from_le_bytes(buf)
    }

    fn next_u64(&mut self) -> u64 {
        let mut buf = [0; 8];
        nrf_softdevice::random_bytes(self.0, &mut buf).unwrap();
        u64::from_le_bytes(buf)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        nrf_softdevice::random_bytes(self.0, dest).unwrap();
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        nrf_softdevice::random_bytes(self.0, dest).map_err(|_| NonZeroU32::new(1).unwrap().into())
    }
}

pub struct FakeSampler(SoftDeviceRng<'static>);

impl FakeSampler {
    pub fn new(sd: &'static Softdevice) -> Self {
        Self(SoftDeviceRng(sd))
    }
}

impl SampleProducerMut for FakeSampler {
    type Output = f32;

    async fn sample(&mut self) -> Sample<Self::Output> {
        use rand::Rng as _;
        //let mut rng = SoftDeviceRng(sd);

        static TIME: Lazy<usize> = Lazy::new(|| 0);
        Timer::after(SAMPLING_INTERVAL).await;
        let timestamp = Instant::now();
        let value = self.0.gen_range(10.0..20.0);
        Sample { timestamp, value }
    }
}
