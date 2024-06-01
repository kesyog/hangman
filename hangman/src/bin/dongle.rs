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
compile_error!("Dongle proto uses nRF52840");

extern crate alloc;

use blocking_hal::Delay as SysTickDelay;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::{
    config::{Config, HfclkSource, LfclkSource},
    gpio::{self, Pin},
    usb::vbus_detect::SoftwareVbusDetect,
};
use embassy_sync::{channel::Channel, mutex::Mutex};
use embassy_time::{Duration, Timer};
use embedded_alloc::Heap;
#[cfg(feature = "console")]
use hangman::console;
use hangman::{
    battery_voltage, ble, blocking_hal,
    button::{self, Button},
    pac, util,
    weight::{self, Hx711},
    MeasureCommandChannel, SharedDelay,
};
use nrf_softdevice::{self as _, SocEvent, Softdevice};
use panic_probe as _;
use static_cell::make_static;

#[global_allocator]
/// Create a small heap. Not sure how to pass around closures without one.
static HEAP: Heap = Heap::empty();
// TODO: how to enforce this in the linker script?
const HEAP_SIZE: usize = 1024;

#[cfg(feature = "console")]
embassy_nrf::bind_interrupts!(struct Irqs {
    USBD => embassy_nrf::usb::InterruptHandler<embassy_nrf::peripherals::USBD>;
    SAADC => embassy_nrf::saadc::InterruptHandler;
});

#[cfg(not(feature = "console"))]
embassy_nrf::bind_interrupts!(struct Irqs {
    SAADC => embassy_nrf::saadc::InterruptHandler;
});

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice, usb_detect: &'static SoftwareVbusDetect) -> ! {
    defmt::debug!("Starting softdevice task");
    sd.run_with_callback(|event| {
        defmt::debug!("SD event: {}", event);
        match event {
            SocEvent::PowerUsbPowerReady => usb_detect.ready(),
            SocEvent::PowerUsbDetected => usb_detect.detected(true),
            SocEvent::PowerUsbRemoved => usb_detect.detected(false),
            _ => (),
        };
    })
    .await
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
        // Reset certain GPIO settings that are retained through system OFF and interfere with the
        // HAL
        util::disable_all_gpio_sense();
    }
    weight::init(weight::Config {
        sampling_interval_hz: 10,
    });

    // This will reset the GPIO latch signal
    let p = embassy_nrf::init(config());
    let syst = pac::CorePeripherals::take().unwrap().SYST;
    let delay: &'static SharedDelay = make_static!(Mutex::new(SysTickDelay::new(syst)));

    // For debugging
    let _green_led = gpio::Output::new(
        p.P0_06.degrade(),
        gpio::Level::High,
        gpio::OutputDrive::Standard,
    );

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

    // USB setup
    // Hack: pretend USB is already connected. not a bad assumption since this is a dongle
    // There might be a race condition at startup between USB init and SD init.
    let usb_detect_ref: &SoftwareVbusDetect = make_static!(SoftwareVbusDetect::new(true, true));

    let sd = ble::init_softdevice();
    spawner.must_spawn(softdevice_task(sd, usb_detect_ref));

    // It's recommended to start the SoftDevice before doing anything else
    embassy_futures::yield_now().await;

    // Enable DC-DC converter for power savings. This is okay since the softdevice has been enabled
    // and the dongle has the necessary inductors
    unsafe {
        nrf_softdevice::raw::sd_power_dcdc_mode_set(
            nrf_softdevice::raw::NRF_POWER_DCDC_MODES_NRF_POWER_DCDC_ENABLE as u8,
        )
    };

    #[cfg(feature = "console")]
    let (usb, class) = console::board::setup_usb(p.USBD, Irqs, usb_detect_ref);

    let ch: &MeasureCommandChannel = make_static!(Channel::new());
    spawner.must_spawn(weight::task_function(ch.receiver(), hx711, sd));
    // Sample battery voltage while sampling to get a reading under load
    ch.sender()
        .send(weight::Command::StartSampling(weight::SampleType::Raw(
            None,
        )))
        .await;
    let battery_voltage = battery_voltage::one_time_sample(p.SAADC, Irqs).await;
    defmt::info!("Battery voltage: {=u32} mV", battery_voltage);
    ch.sender().send(weight::Command::StopSampling).await;

    #[cfg(feature = "console")]
    {
        spawner.must_spawn(console::task::usb_task(usb));
        spawner.must_spawn(console::task::echo_task(class));
    }

    ch.sender().send(weight::Command::Tare).await;
    // Allow time for tare to complete before starting advertising
    // TODO: make this deterministic
    Timer::after(Duration::from_millis(1000)).await;

    // Use user button for wakeup
    let wakeup_button = Button::new(p.P1_06.degrade(), button::Polarity::ActiveLow, true);
    spawner.must_spawn(ble::task_fn(sd, ch.sender(), wakeup_button));

    loop {
        core::future::pending::<()>().await;
    }
}
