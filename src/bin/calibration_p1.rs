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

#![no_main]
#![cfg_attr(not(test), no_std)]
#![feature(type_alias_impl_trait)]
#![feature(async_fn_in_trait)]
#![forbid(unsafe_op_in_unsafe_fn)]

#[cfg(not(feature = "nrf52832"))]
compile_error!("Proto 1.0 uses nRF52832");

extern crate alloc;

use alloc::boxed::Box;
use blocking_hal::Delay as SysTickDelay;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::{
    config::{Config, HfclkSource, LfclkSource},
    gpio::{self, Pin},
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Channel, mutex::Mutex};
use embassy_time::{Duration, Timer};
use embedded_alloc::Heap;
use hangman::{
    blocking_hal, gatt, pac,
    weight::{self, average, Ads1230},
};
use nrf_softdevice::{self as _, Softdevice};
use panic_probe as _;
use static_cell::make_static;

type SharedDelay = Mutex<NoopRawMutex, SysTickDelay>;

#[allow(dead_code)]
enum Mode {
    Calibration,
    CheckCalibration,
}

const SAMPLING_RATE_HZ: usize = 80;

#[global_allocator]
/// Create a small heap. Not sure how to pass around closures without one.
static HEAP: Heap = Heap::empty();
// TODO: how to enforce this in the linker script?
const HEAP_SIZE: usize = 1024;

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    defmt::info!("Starting softdevice task");
    sd.run().await
}

fn config() -> Config {
    // Interrupt priority levels 0, 1, and 4 are reserved for the SoftDevice
    let mut config = Config::default();
    config.hfclk_source = HfclkSource::ExternalXtal;
    config.lfclk_source = LfclkSource::ExternalXtal;
    config.gpiote_interrupt_priority = embassy_nrf::interrupt::Priority::P5;
    config.time_interrupt_priority = embassy_nrf::interrupt::Priority::P5;
    config
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    defmt::println!("Start {}!", core::env!("CARGO_BIN_NAME"));
    unsafe {
        HEAP.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE);
        let reset_reason: u32 = (*pac::POWER::ptr()).resetreas.read().bits();
        defmt::info!("Reset reason: {:X}", reset_reason);
    }
    weight::init(weight::Config {
        sampling_interval_hz: 80,
    });

    let p = embassy_nrf::init(config());
    let syst = pac::CorePeripherals::take().unwrap().SYST;
    let delay: &'static SharedDelay = make_static!(Mutex::new(SysTickDelay::new(syst)));

    let sd = Softdevice::enable(&gatt::softdevice_config());
    spawner.must_spawn(softdevice_task(sd));

    // It's recommended to start the SoftDevice before doing anything else
    embassy_futures::yield_now().await;

    let _vdda_on = gpio::Output::new(
        p.P0_13.degrade(),
        gpio::Level::Low,
        gpio::OutputDrive::Standard,
    );
    let adc_data = gpio::Input::new(p.P0_20.degrade(), gpio::Pull::None);
    let adc_clock = gpio::Output::new(
        p.P0_18.degrade(),
        // Set high initially to power down chip
        gpio::Level::High,
        gpio::OutputDrive::Standard,
    );
    // Not supposed to power up the ADS1230 until at least 10us after the poewr supplies have come
    // up. Insert a delay to be safe.
    Timer::after(Duration::from_micros(10)).await;
    let _pwdn = gpio::Output::new(
        p.P0_03.degrade(),
        gpio::Level::High,
        gpio::OutputDrive::Standard,
    );
    let mut adc = Ads1230::new(adc_data, adc_clock, delay);
    adc.schedule_offset_calibration().await;

    let ch: &hangman::MeasureCommandChannel = make_static!(Channel::new());
    spawner.must_spawn(weight::task_function(ch.receiver(), adc, sd));

    let mut button = gpio::Input::new(p.P0_09.degrade(), gpio::Pull::Up);
    let button_sender = ch.sender();
    let mut active = false;

    let mode = Mode::Calibration;

    loop {
        button.wait_for_falling_edge().await;
        defmt::debug!("button press");
        if active {
            button_sender.send(weight::Command::StopSampling).await;
        } else {
            let cmd = match mode {
                Mode::Calibration => {
                    let mut average = average::Window::<i32>::new(SAMPLING_RATE_HZ);
                    weight::Command::StartSampling(weight::SampleType::FilteredRaw(Some(Box::new(
                        move |_, value| {
                            if let Some(average) = average.add_sample(value) {
                                defmt::info!("Averaged: {}", average);
                            }
                        },
                    ))))
                }
                Mode::CheckCalibration => {
                    let mut average = average::Window::<f32>::new(SAMPLING_RATE_HZ);
                    weight::Command::StartSampling(weight::SampleType::Calibrated(Some(Box::new(
                        move |_, value| {
                            if let Some(average) = average.add_sample(value) {
                                // defmt::info!("Averaged: {}", average);
                                defmt::info!("Averaged: {}", average / 0.454);
                            }
                        },
                    ))))
                }
            };
            button_sender.send(cmd).await;
        }
        active = !active;
    }
}
