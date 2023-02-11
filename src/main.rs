#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]

use embassy_executor::Spawner;
use embassy_nrf::{
    config::{Config, HfclkSource, LfclkSource},
    gpio::{self, Output},
    interrupt,
    peripherals::{self, P0_06},
    usb::{Driver, HardwareVbusDetect},
};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use embassy_usb::UsbDevice;
use nrf_softdevice as _;
use panic_abort as _;
use static_cell::StaticCell;

// Note: HardwareVbusDetect is incompatible with the SoftDevice
type MyDriver = Driver<'static, peripherals::USBD, HardwareVbusDetect>;

#[embassy_executor::task]
async fn usb_task(mut device: UsbDevice<'static, MyDriver>) {
    device.run().await;
}

#[embassy_executor::task]
async fn echo_task(mut class: CdcAcmClass<'static, MyDriver>, mut led: Output<'static, P0_06>) {
    loop {
        class.wait_connection().await;
        led.set_high();
        let _ = echo(&mut class).await;
    }
}

struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

async fn echo(class: &mut CdcAcmClass<'static, MyDriver>) -> Result<(), Disconnected> {
    let mut buf = [0; 64];
    loop {
        let n = class.read_packet(&mut buf).await?;
        let data = &buf[..n];
        class.write_packet(data).await?;
    }
}

fn config() -> Config {
    // Interrupt priority levels 0, 1, and 4 are reserved for the SoftDevice
    let mut config = Config::default();
    config.hfclk_source = HfclkSource::ExternalXtal;
    config.lfclk_source = LfclkSource::ExternalXtal;
    cfg_if::cfg_if! {
        if #[cfg(feature = "gpiote")] {
            config.gpiote_interrupt_priority = embassy_nrf::interrupt::Priority::P5;
        }
    }
    cfg_if::cfg_if! {
        if #[cfg(feature = "_time-driver")] {
        config.time_interrupt_priority: embassy_nrf::interrupt::Priority: P5;
        }
    }
    config
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let p = embassy_nrf::init(config());
    let ld1 = gpio::Output::new(p.P0_06, gpio::Level::Low, gpio::OutputDrive::Standard);

    // USB setup
    // Create the driver, from the HAL.
    let irq = interrupt::take!(USBD);
    let power_irq = interrupt::take!(POWER_CLOCK);
    let driver = Driver::new(p.USBD, irq, HardwareVbusDetect::new(power_irq));

    // Create embassy-usb Config
    let mut config = embassy_usb::Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Kes LLC");
    config.product = Some("KesOMatic");
    config.serial_number = Some("deadbeef");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    // Required for windows compatiblity.
    // https://developer.nordicsemi.com/nRF_Connect_SDK/doc/1.9.1/kconfig/CONFIG_CDC_ACM_IAD.html#help
    //config.device_class = 0x02;
    //config.device_sub_class = 0x02;
    /*
    config.device_protocol = 0x01;
    config.composite_with_iads = true;
    */

    struct Resources {
        device_descriptor: [u8; 256],
        config_descriptor: [u8; 256],
        bos_descriptor: [u8; 256],
        control_buf: [u8; 64],
        serial_state: State<'static>,
    }
    static RESOURCES: StaticCell<Resources> = StaticCell::new();
    let res = RESOURCES.init(Resources {
        device_descriptor: [0; 256],
        config_descriptor: [0; 256],
        bos_descriptor: [0; 256],
        control_buf: [0; 64],
        serial_state: State::new(),
    });

    // Create embassy-usb DeviceBuilder using the driver and config.
    let mut builder = embassy_usb::Builder::new(
        driver,
        config,
        &mut res.device_descriptor,
        &mut res.config_descriptor,
        &mut res.bos_descriptor,
        &mut res.control_buf,
    );

    // Create classes on the builder.
    let class = CdcAcmClass::new(&mut builder, &mut res.serial_state, 64);

    // Build the builder.
    let usb = builder.build();
    spawner.must_spawn(usb_task(usb));
    spawner.must_spawn(echo_task(class, ld1));

    let mut button = gpio::Input::new(p.P1_06, gpio::Pull::Up);
    let mut blue_led = gpio::Output::new(p.P0_12, gpio::Level::Low, gpio::OutputDrive::Standard);

    loop {
        button.wait_for_falling_edge().await;
        let led_state: bool = blue_led.get_output_level().into();
        blue_led.set_level((!led_state).into());
    }
}
