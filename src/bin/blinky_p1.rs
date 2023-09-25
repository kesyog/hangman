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

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::{
    config::{Config, Debug, HfclkSource, LfclkSource},
    gpio::{self, Pin},
};
use embassy_time::{Duration, Timer};
use embedded_alloc::Heap;
use hangman::{gatt, pac};
use nrf_softdevice::{self as _, Softdevice};
use panic_probe as _;
// use static_cell::make_static;

#[allow(dead_code)]
enum Mode {
    Calibration,
    CheckCalibration,
}

// Pins
// ADC_DATA: P0.20
// ADC_CLOCK: P0.18
// /VDDA_ON: P0.13
// /PWDN: P0.03
// SW1: P0.09
// SW2: P0.10
// /LED: P0.02

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

    let p = embassy_nrf::init(config());

    let sd = Softdevice::enable(&gatt::softdevice_config());
    spawner.must_spawn(softdevice_task(sd));

    // It's recommended to start the SoftDevice before doing anything else
    embassy_futures::yield_now().await;

    let mut led = gpio::Output::new(
        p.P0_02.degrade(),
        gpio::Level::High,
        gpio::OutputDrive::Standard,
    );

    defmt::info!("Toggling LED");
    loop {
        Timer::after(Duration::from_millis(1000)).await;
        led.set_high();
        Timer::after(Duration::from_millis(1000)).await;
        led.set_low();
    }
}
