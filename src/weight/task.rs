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
use super::hx711::Hx711;
use super::tare::Tarer;
use super::{average, median::Median, Command, Sample, SampleProducerMut, SampleType};
use crate::nonvolatile::Nvm;
use crate::MeasureCommandReceiver;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant, Timer};
use fix_hidden_lifetime_bug::fix_hidden_lifetime_bug;
use nrf_softdevice::Softdevice;
use static_cell::make_static;

const THREAD_SLEEP_DELAY: Duration = Duration::from_millis(100);

type SharedAdc = Mutex<NoopRawMutex, Hx711<'static>>;
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
}

async fn handle_command(cmd: Command, context: &mut MeasurementContext, adc: &SharedAdc) {
    match cmd {
        Command::StartSampling(measurement_cb) => {
            // TODO: check state before doing anything
            {
                /*
                let mut leds = crate::leds::singleton_get().lock().await;
                leds.rgb_red.set_low();
                */
            }
            adc.lock().await.power_up().await;
            context.state = MeasurementState::Active(measurement_cb, Instant::now());
        }
        Command::StopSampling => {
            {
                /*
                let mut leds = crate::leds::singleton_get().lock().await;
                leds.rgb_red.set_high();
                */
            }
            adc.lock().await.power_down();
            context.state = MeasurementState::Idle;
        }
        Command::Tare => {
            const WARMUP: usize = 80;
            const FILTER_SIZE: usize = 80;
            for _ in 0..WARMUP {
                let _ = context.calibrator.sample().await;
            }
            let mut filter = average::Window::<f32>::new(FILTER_SIZE);
            for _ in 0..(FILTER_SIZE - 1) {
                let Sample { value, .. } = context.calibrator.sample().await;
                assert!(filter.add_sample(value).is_none());
            }
            let Sample { value, .. } = context.calibrator.sample().await;
            let average = filter.add_sample(value).unwrap();
            context.tarer.set_offset(average);

            adc.lock().await.power_down();
            context.state = MeasurementState::Idle;
        }
    }
}

// Workaround for Rust compiler bug
// See https://github.com/danielhenrymantilla/fix_hidden_lifetime_bug.rs
#[allow(clippy::manual_async_fn)]
#[fix_hidden_lifetime_bug]
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
pub async fn task_function(
    rx: MeasureCommandReceiver,
    adc: Hx711<'static>,
    sd: &'static Softdevice,
) {
    defmt::debug!("Starting measurement task");
    let adc: &SharedAdc = make_static!(Mutex::new(adc));
    let median: &'static SharedFilteredAdc = make_static!(Mutex::new(Median::new(adc)));

    let nvm = Nvm::new(sd);
    let cal_m = nvm.read_cal_m();
    let cal_b = nvm.read_cal_b();
    defmt::info!("Loaded calibration: m={} b={}", cal_m, cal_b);
    let calibrator: &SharedCalibrator =
        make_static!(Mutex::new(Calibrator::new(median, cal_m, cal_b)));

    let tarer = Tarer::new(calibrator);
    let mut context = MeasurementContext {
        state: MeasurementState::Idle,
        adc,
        median,
        calibrator,
        tarer,
    };

    loop {
        if let Ok(cmd) = rx.try_recv() {
            defmt::info!("Measure task received {} command", cmd);
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
