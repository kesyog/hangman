#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]

use embassy_executor::Spawner;
use embassy_nrf as hal;

use hal::{
    config::{Config, HfclkSource, LfclkSource},
    gpio,
};
use nrf_softdevice as _;
use panic_abort as _;

fn config() -> Config {
    // Interrupt priority levels 0, 1, and 4 are reserved for the SoftDevice
    let mut config = Config::default();
    config.hfclk_source = HfclkSource::ExternalXtal;
    config.lfclk_source = LfclkSource::ExternalXtal;
    cfg_if::cfg_if! {
        if #[cfg(feature = "gpiote")] {
            config.gpiote_interrupt_priority = hal::interrupt::Priority::P5;
        }
    }
    cfg_if::cfg_if! {
        if #[cfg(feature = "_time-driver")] {
        config.time_interrupt_priority: hal::interrupt::Priority: P5;
        }
    }
    config
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let p = hal::init(config());
    let button = gpio::Input::new(p.P1_06, gpio::Pull::Up);
    let mut blue_led = gpio::Output::new(p.P0_12, gpio::Level::High, gpio::OutputDrive::Standard);

    loop {
        if button.is_high() {
            blue_led.set_high();
        } else {
            blue_led.set_low();
        }
    }
}
