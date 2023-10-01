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
use embassy_sync::{channel::Channel, mutex::Mutex};
use embassy_time::{Duration, Timer};
use embedded_alloc::Heap;
use hangman::{
    battery_voltage, blocking_hal,
    button::{self, Button},
    gatt, pac, sleep, util,
    weight::{self, Ads1230},
    MeasureCommandChannel, SharedDelay,
};
use nrf_softdevice::{self as _, Softdevice};
use panic_probe as _;
use static_cell::make_static;

#[global_allocator]
/// Create a small heap. Not sure how to pass around closures without one.
static HEAP: Heap = Heap::empty();
// TODO: how to enforce this in the linker script?
const HEAP_SIZE: usize = 1024;

embassy_nrf::bind_interrupts!(struct Irqs {
    SAADC => embassy_nrf::saadc::InterruptHandler;
});

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    defmt::debug!("Starting softdevice task");
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

// Pins
// ADC_DATA: P0.20
// ADC_CLOCK: P0.18
// /VDDA_ON: P0.13
// /PWDN: P0.03
// SW1: P0.09 (power switch)
// SW2: P0.10
// /LED: P0.26

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    defmt::println!("Start {}!", core::env!("CARGO_BIN_NAME"));
    unsafe {
        HEAP.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE);
        let reset_reason: u32 = (*pac::POWER::ptr()).resetreas.read().bits();
        defmt::info!("Reset reason: {:X}", reset_reason);
        // Reset certain GPIO settings that are retained through system OFF and interfere with the
        // HAL
        util::disable_all_gpio_sense();
    }
    weight::init(weight::Config {
        sampling_interval_hz: 80,
    });

    let p = embassy_nrf::init(config());
    let syst = pac::CorePeripherals::take().unwrap().SYST;
    let delay: &'static SharedDelay = make_static!(Mutex::new(SysTickDelay::new(syst)));

    let sd = Softdevice::enable(&gatt::softdevice_config());
    gatt::server::init(sd).unwrap();
    spawner.must_spawn(softdevice_task(sd));

    // It's recommended to start the SoftDevice before doing anything else
    embassy_futures::yield_now().await;

    // Enable DC-DC converter for power savings. This is okay since the softdevice has been enabled
    // and the BC832 module has the necessary inductors
    unsafe {
        nrf_softdevice::raw::sd_power_dcdc_mode_set(
            nrf_softdevice::raw::NRF_POWER_DCDC_MODES_NRF_POWER_DCDC_ENABLE as u8,
        )
    };

    // Enable analog supply and ADC
    let vdda_on = gpio::Output::new(
        p.P0_13.degrade(),
        gpio::Level::High,
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
    let mut pwdn = gpio::Output::new(
        p.P0_03.degrade(),
        gpio::Level::High,
        gpio::OutputDrive::Standard,
    );

    sleep::register_system_off_callback(Box::new(move || {
        // Power down ADC
        pwdn.set_low();
    }));

    let mut adc = Ads1230::new(adc_data, adc_clock, vdda_on, delay);
    adc.schedule_offset_calibration().await;

    let ch: &MeasureCommandChannel = make_static!(Channel::new());
    spawner.must_spawn(weight::task_function(ch.receiver(), adc, sd));

    // Sample battery voltage while sampling to get a reading under load
    ch.sender()
        .send(weight::Command::StartSampling(weight::SampleType::Raw(
            None,
        )))
        .await;
    let battery_voltage = battery_voltage::one_time_sample(p.SAADC, Irqs).await;
    defmt::info!("Battery voltage: {} mV", battery_voltage);
    ch.sender().send(weight::Command::StopSampling).await;

    // This will run the offset calibration that we scheduled above
    ch.sender().send(weight::Command::Tare).await;
    // Allow time for tare to complete before starting advertising
    // TODO: make this deterministic
    Timer::after(Duration::from_millis(1000)).await;

    // Use SW1 = power button for wakeup
    let wakeup_button = Button::new(p.P0_09.degrade(), button::Polarity::ActiveLow, true);
    spawner.must_spawn(gatt::ble_task(sd, ch.sender(), wakeup_button));

    loop {
        core::future::pending::<()>().await;
    }
}
