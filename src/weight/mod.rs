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

pub mod average;
mod calibrate;
pub mod hx711;
pub mod median;
mod random;
mod tare;
mod task;

extern crate alloc;

use crate::nonvolatile::Nvm;
use alloc::boxed::Box;
use core::ops::DerefMut;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant};
pub use task::task_function;

const SAMPLING_INTERVAL: Duration = Duration::from_hz(80);
// Temporary defaults for test load cell
pub const DEFAULT_CALIBRATION_M: f32 = 4.6750380809321235e-06;
pub const DEFAULT_CALIBRATION_B: i32 = -100598;

pub type OnRawMeasurementCb = dyn FnMut(Duration, i32);
pub type OnCalibratedMeasurementCb = dyn FnMut(Duration, f32);
pub type OnTaredMeasurementCb = dyn FnMut(Duration, f32);

pub enum SampleType {
    Raw(Option<Box<OnRawMeasurementCb>>),
    FilteredRaw(Option<Box<OnRawMeasurementCb>>),
    Calibrated(Option<Box<OnCalibratedMeasurementCb>>),
    Tared(Option<Box<OnTaredMeasurementCb>>),
}

pub enum Command {
    /// Start measuring continuously
    StartSampling(SampleType),
    StopSampling,
    Tare,
}

impl defmt::Format for Command {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            Command::StartSampling(SampleType::Raw(_)) => defmt::write!(fmt, "StartSampling (Raw)"),
            Command::StartSampling(SampleType::FilteredRaw(_)) => {
                defmt::write!(fmt, "StartSampling (FilteredRaw)")
            }
            Command::StartSampling(SampleType::Calibrated(_)) => {
                defmt::write!(fmt, "StartSampling (Calibrated)");
            }
            Command::StartSampling(SampleType::Tared(_)) => {
                defmt::write!(fmt, "StartSampling (Tared)");
            }
            Command::StopSampling => defmt::write!(fmt, "StopSampling"),
            Command::Tare => defmt::write!(fmt, "Tare"),
        }
    }
}

async fn write_calibration(nvm: &mut Nvm, cal_m: f32, cal_b: i32) {
    nvm.write_cal_m(cal_m);
    nvm.write_cal_b(cal_b);
    nvm.flush().await;
}

pub struct Sample<T> {
    pub timestamp: Instant,
    pub value: T,
}

pub trait SampleProducerMut {
    type Output;

    async fn sample(&mut self) -> Sample<Self::Output>;
}

pub trait SampleProducer {
    type Output;

    async fn sample(&self) -> Sample<Self::Output>;
}

impl<T> SampleProducerMut for T
where
    T: SampleProducer,
{
    type Output = T::Output;

    async fn sample(&mut self) -> Sample<Self::Output> {
        SampleProducer::sample(self).await
    }
}

impl<T, M> SampleProducer for Mutex<M, T>
where
    T: SampleProducerMut,
    M: RawMutex,
{
    type Output = T::Output;

    async fn sample(&self) -> Sample<Self::Output> {
        let mut producer = self.lock().await;
        SampleProducerMut::sample(DerefMut::deref_mut(&mut producer)).await
    }
}

impl<T, M> SampleProducer for &Mutex<M, T>
where
    T: SampleProducerMut,
    M: RawMutex,
{
    type Output = T::Output;

    async fn sample(&self) -> Sample<<T as SampleProducerMut>::Output> {
        let mut producer = self.lock().await;
        SampleProducerMut::sample(DerefMut::deref_mut(&mut producer)).await
    }
}
