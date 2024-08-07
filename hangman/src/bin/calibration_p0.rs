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
#![forbid(unsafe_op_in_unsafe_fn)]

#[cfg(not(feature = "nrf52840"))]
compile_error!("Proto 0.0 uses nRF52840");

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
use embedded_alloc::Heap;
use hangman::{
    ble, blocking_hal, make_static, pac,
    weight::{self, average, Hx711},
};
use nrf_softdevice::{self as _, Softdevice};
use panic_probe as _;

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
    defmt::println!("Start {=str}!", core::env!("CARGO_BIN_NAME"));
    unsafe {
        HEAP.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE);
        let reset_reason: u32 = (*pac::POWER::ptr()).resetreas.read().bits();
        defmt::info!("Reset reason: {=u32:X}", reset_reason);
    }
    weight::init(weight::Config {
        sampling_interval_hz: 80,
    });

    let p = embassy_nrf::init(config());
    let syst = pac::CorePeripherals::take().unwrap().SYST;
    let delay: &'static SharedDelay =
        make_static!(SharedDelay, Mutex::new(SysTickDelay::new(syst)));

    let sd = ble::init_softdevice();
    spawner.must_spawn(softdevice_task(sd));

    // It's recommended to start the SoftDevice before doing anything else
    embassy_futures::yield_now().await;

    // orange DATA 0.17
    let hx711_data = gpio::Input::new(p.P0_17.degrade(), gpio::Pull::None);
    // yellow CLK 0.20
    let hx711_clock = gpio::Output::new(
        p.P0_20.degrade(),
        // Set high initially to power down chip
        gpio::Level::High,
        gpio::OutputDrive::Standard,
    );
    let hx711 = Hx711::new(hx711_data, hx711_clock, delay);

    let ch: &hangman::MeasureCommandChannel =
        make_static!(hangman::MeasureCommandChannel, Channel::new());
    spawner.must_spawn(weight::task_function(ch.receiver(), hx711, sd));

    let mut button = gpio::Input::new(p.P1_06.degrade(), gpio::Pull::Up);
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
                                defmt::info!("Averaged: {=i32}", average);
                            }
                        },
                    ))))
                }
                Mode::CheckCalibration => {
                    let mut average = average::Window::<f32>::new(SAMPLING_RATE_HZ);
                    weight::Command::StartSampling(weight::SampleType::Calibrated(Some(Box::new(
                        move |_, value| {
                            if let Some(average) = average.add_sample(value) {
                                defmt::info!("Averaged: {=f32}", average / 0.454);
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
