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

use super::calibrate::Calibrator;
use super::tare::Tarer;
#[cfg(feature = "nrf52832")]
use super::Ads1230;
#[cfg(feature = "nrf52840")]
use super::Hx711;
use super::{average, median::Median, Command, RawReading, Sample, SampleProducerMut, SampleType};
use crate::{make_static, nonvolatile::Nvm, MeasureCommandReceiver};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant, Timer};
use hangman_utils::two_point_cal::{self, CalPoint, TwoPoint};
use nrf_softdevice::Softdevice;

const THREAD_SLEEP_DELAY: Duration = Duration::from_millis(100);

#[cfg(feature = "nrf52832")]
type Adc = Ads1230<'static>;
#[cfg(feature = "nrf52840")]
type Adc = Hx711<'static>;

type SharedAdc = Mutex<NoopRawMutex, Adc>;
type SharedFilteredAdc = Mutex<NoopRawMutex, Median<&'static SharedAdc>>;
type SharedCalibrator = Mutex<NoopRawMutex, Calibrator<&'static SharedFilteredAdc>>;

enum MeasurementState {
    Idle,
    Active(SampleType, Instant),
}

struct MeasurementContext {
    state: MeasurementState,
    adc: &'static SharedAdc,
    median: &'static SharedFilteredAdc,
    calibrator: &'static SharedCalibrator,
    tarer: Tarer<&'static SharedCalibrator>,
    nvm: Nvm,
    factory_cal: TwoPoint<RawReading>,
}

async fn handle_command(cmd: Command, context: &mut MeasurementContext, adc: &SharedAdc) {
    match cmd {
        Command::StartSampling(measurement_cb) => {
            if !matches!(context.state, MeasurementState::Idle) {
                defmt::error!("Can't start sampling while already measuring");
                return;
            }
            adc.lock().await.power_up().await;
            context.state = MeasurementState::Active(measurement_cb, Instant::now());
        }
        Command::StopSampling => {
            adc.lock().await.power_down();
            context.state = MeasurementState::Idle;
        }
        Command::Tare => {
            if !matches!(context.state, MeasurementState::Idle) {
                defmt::error!("Can't tare while measuring");
                return;
            }

            // 0.5 second
            let warmup = super::sampling_interval_hz() / 2;
            // 0.5 second
            let filter_size = super::sampling_interval_hz() / 2;
            for _ in 0..warmup {
                let _ = context.calibrator.sample().await;
            }
            let mut filter = average::Window::<f32>::new(filter_size);
            for _ in 0..(filter_size - 1) {
                let Sample { value, .. } = context.calibrator.sample().await;
                assert!(filter.add_sample(value).is_none());
            }
            let Sample { value, .. } = context.calibrator.sample().await;
            let average = filter.add_sample(value).unwrap();
            context.tarer.set_offset(average);

            adc.lock().await.power_down();
            context.state = MeasurementState::Idle;
        }
        Command::AddCalibrationPoint(weight) => {
            if !matches!(context.state, MeasurementState::Idle) {
                defmt::error!("Can't run factory cal while measuring");
                return;
            }

            // 1 second
            let warmup = super::sampling_interval_hz();
            // 1 second
            let filter_size = super::sampling_interval_hz();
            for _ in 0..warmup {
                let _ = context.calibrator.sample().await;
            }
            let mut filter = average::Window::<RawReading>::new(filter_size);
            for _ in 0..(filter_size - 1) {
                let Sample { value, .. } = context.median.sample().await;
                assert!(filter.add_sample(value).is_none());
            }
            let Sample { value, .. } = context.median.sample().await;
            let reading = filter.add_sample(value).unwrap();
            context.factory_cal.add_point(CalPoint {
                expected_value: weight,
                measured_value: reading,
            });
        }
        Command::SaveCalibration => {
            if let Some(two_point_cal::Constants { m, b }) = context.factory_cal.get_cal_constants()
            {
                defmt::info!("New calibration: m = {=f32} b = {=i32}", m, b);
                super::write_calibration(&mut context.nvm, m, b).await;
                context.calibrator.lock().await.set_calibration(m, b);
            } else {
                defmt::error!("Not enough data points to calibrate");
            }
        }
    }
}

async fn measure(context: &mut MeasurementContext) {
    let MeasurementState::Active(ref mut sample_type, ref mut start_time) = context.state else {
        return;
    };
    let mut calculate_duration =
        |timestamp: Instant| match timestamp.checked_duration_since(*start_time) {
            Some(duration) => duration,
            None => {
                *start_time = timestamp;
                Duration::from_ticks(0)
            }
        };
    match sample_type {
        SampleType::Raw(cb) => {
            let Sample { timestamp, value } = context.adc.sample().await;
            if let Some(cb) = cb {
                cb(calculate_duration(timestamp), value);
            }
        }
        SampleType::FilteredRaw(cb) => {
            let Sample { timestamp, value } = context.median.sample().await;
            if let Some(cb) = cb {
                cb(calculate_duration(timestamp), value);
            }
        }
        SampleType::Calibrated(cb) => {
            let Sample { timestamp, value } = context.calibrator.sample().await;
            if let Some(cb) = cb {
                cb(calculate_duration(timestamp), value);
            }
        }
        SampleType::Tared(cb) => {
            let Sample { timestamp, value } = context.tarer.sample().await;
            if let Some(cb) = cb {
                cb(calculate_duration(timestamp), value);
            }
        }
    };
}

#[embassy_executor::task]
pub async fn task_function(rx: MeasureCommandReceiver, adc: Adc, sd: &'static Softdevice) {
    defmt::debug!("Starting measurement task");
    let adc: &SharedAdc = make_static!(SharedAdc, Mutex::new(adc));
    let median: &'static SharedFilteredAdc =
        make_static!(SharedFilteredAdc, Mutex::new(Median::new(adc)));

    let nvm = Nvm::new(sd);
    let cal_m = nvm.read_cal_m();
    let cal_b = nvm.read_cal_b();
    defmt::info!("Loaded calibration: m={=f32} b={=i32}", cal_m, cal_b);
    let calibrator: &SharedCalibrator = make_static!(
        SharedCalibrator,
        Mutex::new(Calibrator::new(median, cal_m, cal_b))
    );

    let tarer = Tarer::new(calibrator);
    let mut context = MeasurementContext {
        state: MeasurementState::Idle,
        adc,
        median,
        calibrator,
        tarer,
        nvm,
        factory_cal: TwoPoint::default(),
    };

    loop {
        if let Ok(cmd) = rx.try_receive() {
            defmt::info!("Measure task received command: {}", cmd);
            handle_command(cmd, &mut context, adc).await;
        }
        if let MeasurementState::Active(..) = context.state {
            measure(&mut context).await;
        } else {
            // Give a chance for other tasks to run
            Timer::after(THREAD_SLEEP_DELAY).await;
        }
    }
}
