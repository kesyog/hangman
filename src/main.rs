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
// TODO: remove once crate is closer to feature-complete
#![allow(dead_code)]

#[cfg(feature = "console")]
mod console;
mod gatt;
mod leds;
mod nonvolatile;
mod weight;

extern crate alloc;

use alloc::boxed::Box;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::{
    config::{Config, HfclkSource, LfclkSource},
    gpio::{self, AnyPin, Pin},
    usb::vbus_detect::SoftwareVbusDetect,
};
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    channel::{Channel, Receiver},
    mutex::Mutex,
};
use embedded_alloc::Heap;
use nrf52840_hal::Delay as SysTickDelay;
use nrf_softdevice::{self as _, SocEvent, Softdevice};
use panic_probe as _;
use static_cell::StaticCell;
use weight::hx711::Hx711;

#[cfg(feature = "console")]
embassy_nrf::bind_interrupts!(struct Irqs {
    USBD => embassy_nrf::usb::InterruptHandler<embassy_nrf::peripherals::USBD>;
});

#[global_allocator]
/// Create a small heap. Not sure how to pass around closures without one.
static HEAP: Heap = Heap::empty();
// TODO: how to enforce this in the linker script?
const HEAP_SIZE: usize = 1024;

// Leave some room for multiple commands to be queued. If this is too small, we can get overwhelmed
// and deadlock.
const MEASURE_COMMAND_CHANNEL_SIZE: usize = 5;

type SharedDelay = Mutex<NoopRawMutex, SysTickDelay>;
type MeasureCommandChannel = Channel<NoopRawMutex, weight::Command, MEASURE_COMMAND_CHANNEL_SIZE>;
type MeasureCommandReceiver =
    Receiver<'static, NoopRawMutex, weight::Command, MEASURE_COMMAND_CHANNEL_SIZE>;

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice, usb_detect: &'static SoftwareVbusDetect) -> ! {
    defmt::info!("Starting softdevice task");
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

fn setup_softdevice() -> &'static mut Softdevice {
    use nrf_softdevice::raw;
    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_XTAL as u8,
            rc_ctiv: 0,
            rc_temp_ctiv: 0,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 2,
            event_length: 24,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t { att_mtu: 256 }),
        gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t {
            attr_tab_size: 2048,
        }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: 2,
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            p_value: (b"Progressor_1719" as *const u8).cast_mut(),
            current_len: 15,
            max_len: 15,
            write_perm: unsafe { core::mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(
                raw::BLE_GATTS_VLOC_STACK as u8,
            ),
        }),
        ..Default::default()
    };
    Softdevice::enable(&config)
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
    defmt::println!("Start!");
    unsafe {
        HEAP.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE);
    }
    let p = embassy_nrf::init(config());
    let syst = embassy_nrf::pac::CorePeripherals::take().unwrap().SYST;
    static DELAY: StaticCell<Mutex<NoopRawMutex, SysTickDelay>> = StaticCell::new();
    let delay: &'static SharedDelay = DELAY.init(Mutex::new(SysTickDelay::new(syst)));
    let green_led = gpio::Output::new(
        AnyPin::from(p.P0_06),
        gpio::Level::High,
        gpio::OutputDrive::Standard,
    );
    let rgb_red_led = gpio::Output::new(
        AnyPin::from(p.P0_08),
        gpio::Level::High,
        gpio::OutputDrive::Standard,
    );
    let rgb_blue_led = gpio::Output::new(
        AnyPin::from(p.P0_12),
        gpio::Level::High,
        gpio::OutputDrive::Standard,
    );
    leds::singleton_init(rgb_blue_led, rgb_red_led, green_led).unwrap();

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
    let usb_detect_ref: &SoftwareVbusDetect = {
        static USB_DETECT: StaticCell<SoftwareVbusDetect> = StaticCell::new();
        // Hack: pretend USB is already connected. not a bad assumption since this is a dongle
        // There might be a race condition at startup between USB init and SD init.
        USB_DETECT.init(SoftwareVbusDetect::new(true, true))
    };

    let sd = setup_softdevice();
    gatt::server::init(sd).unwrap();
    spawner.must_spawn(softdevice_task(sd, usb_detect_ref));

    // It's recommended to start the SoftDevice before doing anything else
    embassy_futures::yield_now().await;

    #[cfg(feature = "console")]
    let (usb, class) = console::board::setup_usb(p.USBD, Irqs, usb_detect_ref);

    static GATT_MEASUREMENT_CHANNEL: StaticCell<MeasureCommandChannel> = StaticCell::new();
    let ch = GATT_MEASUREMENT_CHANNEL.init(Channel::new());

    // Start tasks
    #[cfg(feature = "console")]
    {
        spawner.must_spawn(console::task::usb_task(usb));
        spawner.must_spawn(console::task::echo_task(class));
    }
    spawner.must_spawn(gatt::ble_task(sd, ch.sender()));
    spawner.must_spawn(weight::task_function(ch.receiver(), hx711, sd));

    let mut button = gpio::Input::new(p.P1_06, gpio::Pull::Up);
    let button_sender = ch.sender();
    let mut active = false;

    // Temporary janky flow to calibrate / test calibration
    // Use button to start/stop measurements
    loop {
        button.wait_for_falling_edge().await;
        defmt::debug!("button press");
        if active {
            button_sender.send(weight::Command::StopSampling).await;
        } else {
            // Use WindowAverageInt for raw values or WindowAverageFloat for calibrated values
            let mut average = weight::average::WindowAveragerFloat::<20>::default();
            button_sender
                .send(weight::Command::StartSampling(
                    weight::SampleType::Calibrated(Some(Box::new(move |_, value| {
                        if let Some(average) = average.add_sample(value) {
                            defmt::info!("Averaged: {}", average);
                        }
                    }))),
                ))
                .await;
        }
        active = !active;
    }
}
