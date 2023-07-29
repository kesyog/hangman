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
use super::{Command, Sample, SampleProducerMut, SampleType};
use crate::nonvolatile::Nvm;
use crate::MeasureCommandReceiver;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant, Timer};
use fix_hidden_lifetime_bug::fix_hidden_lifetime_bug;
use nrf_softdevice::Softdevice;
use static_cell::StaticCell;

const THREAD_SLEEP_DELAY: Duration = Duration::from_millis(100);

type SharedAdc = Mutex<NoopRawMutex, Hx711<'static>>;
type SharedCalibrator = Mutex<NoopRawMutex, Calibrator<&'static SharedAdc>>;

enum MeasurementState {
    Idle,
    Active(SampleType, Instant),
}

struct MeasurementContext {
    state: MeasurementState,
    adc: &'static SharedAdc,
    calibrator: &'static SharedCalibrator,
    tarer: Tarer<&'static SharedCalibrator>,
}

async fn handle_command(cmd: Command, context: &mut MeasurementContext, adc: &SharedAdc) {
    match cmd {
        Command::StartSampling(measurement_cb) => {
            // TODO: check state before doing anything
            {
                let mut leds = crate::leds::singleton_get().lock().await;
                leds.rgb_red.set_low();
            }
            adc.lock().await.power_up();
            context.state = MeasurementState::Active(measurement_cb, Instant::now());
        }
        Command::StopSampling => {
            {
                let mut leds = crate::leds::singleton_get().lock().await;
                leds.rgb_red.set_high();
            }
            adc.lock().await.power_down();
            context.state = MeasurementState::Idle;
        }
        Command::Tare => {
            let Sample { value, .. } = context.calibrator.sample().await;
            // TODO: add filtering
            context.tarer.set_offset(-value);
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
    //measurement_cb(duration_since_start, value);
}

#[embassy_executor::task]
pub async fn task_function(
    rx: MeasureCommandReceiver,
    adc: Hx711<'static>,
    sd: &'static Softdevice,
) {
    defmt::info!("Starting measurement task");
    static HX711: StaticCell<SharedAdc> = StaticCell::new();
    let adc: &SharedAdc = HX711.init(Mutex::new(adc));

    static CALIBRATOR: StaticCell<Mutex<NoopRawMutex, Calibrator<&'static SharedAdc>>> =
        StaticCell::new();
    let nvm = Nvm::new(sd);
    let cal_m = nvm.read_cal_m();
    let cal_b = nvm.read_cal_b();
    defmt::info!("Loaded calibration: m={} b={}", cal_m, cal_b);
    let calibrator: &SharedCalibrator =
        CALIBRATOR.init(Mutex::new(Calibrator::new(adc, cal_m, cal_b)));

    let tarer = Tarer::new(calibrator);
    let mut context = MeasurementContext {
        state: MeasurementState::Idle,
        adc,
        calibrator,
        tarer,
    };

    loop {
        if let Ok(cmd) = rx.try_recv() {
            defmt::debug!("Measure task received {}", cmd);
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
