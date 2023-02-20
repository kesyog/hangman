use core::num::NonZeroU32;

use crate::gatt::DataOpcode;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Receiver;
use embassy_time::{Duration, Instant, Timer};
use nrf_softdevice::ble::Connection;
use nrf_softdevice::Softdevice;
use once_cell::sync::Lazy;
use rand::RngCore;

const SAMPLING_INTERVAL: Duration = Duration::from_hz(80);

type ReceiveChannel = Receiver<'static, NoopRawMutex, Command, 1>;

pub enum Command {
    StartMeasurement(Connection),
    StopMeasurement,
    Tare,
}

impl defmt::Format for Command {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            Command::StartMeasurement(_) => defmt::write!(fmt, "StartMeasurement"),
            Command::StopMeasurement => defmt::write!(fmt, "StopMeasurement"),
            Command::Tare => defmt::write!(fmt, "Tare"),
        }
    }
}

#[derive(Clone)]
enum MeasurementState {
    Idle,
    Active(Connection, Instant),
}

#[derive(Clone)]
struct MeasurementContext {
    state: MeasurementState,
    offset: f32,
}

async fn wait_for_command(
    rx: &ReceiveChannel,
    context: &MeasurementContext,
    sd: &'static Softdevice,
) -> MeasurementContext {
    let mut context = context.clone();
    let rx_cmd = rx.recv().await;
    defmt::println!("measure received {}", rx_cmd);
    match rx_cmd {
        Command::StartMeasurement(connection) => {
            context.state = MeasurementState::Active(connection, Instant::now())
        }
        Command::StopMeasurement => context.state = MeasurementState::Idle,
        Command::Tare => {
            let measurement = take_measurement(sd).await;
            context.offset -= measurement;
        }
    }
    context
}

async fn measure(context: &MeasurementContext, sd: &'static Softdevice) {
    let MeasurementState::Active(ref connection, start_time) = context.state else {
        // Sleep to give a chance for other tasks to run
        Timer::after(Duration::from_millis(100)).await;
        return;
    };
    let measurement = take_measurement(sd).await + context.offset;
    let timestamp = Instant::now().duration_since(start_time).as_micros() as u32;
    if crate::gatt::notify_data(DataOpcode::Weight(measurement, timestamp), connection).is_err() {
        defmt::error!("Notify failed");
    };
}

#[embassy_executor::task]
pub async fn measure_task(sd: &'static Softdevice, rx: ReceiveChannel) {
    defmt::println!("starting measurement task");
    let mut context = MeasurementContext {
        state: MeasurementState::Idle,
        offset: 0.0,
    };

    loop {
        let result = select(wait_for_command(&rx, &context, sd), measure(&context, sd)).await;
        match result {
            Either::First(new_ctx) => context = new_ctx,
            Either::Second(_) => (),
        }
    }
}

struct SoftDeviceRng<'a>(&'a Softdevice);

impl<'a> RngCore for SoftDeviceRng<'a> {
    fn next_u32(&mut self) -> u32 {
        let mut buf = [0; 4];
        nrf_softdevice::random_bytes(self.0, &mut buf).unwrap();
        u32::from_le_bytes(buf)
    }

    fn next_u64(&mut self) -> u64 {
        let mut buf = [0; 8];
        nrf_softdevice::random_bytes(self.0, &mut buf).unwrap();
        u64::from_le_bytes(buf)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        nrf_softdevice::random_bytes(self.0, dest).unwrap();
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        nrf_softdevice::random_bytes(self.0, dest).map_err(|_| NonZeroU32::new(1).unwrap().into())
    }
}

async fn take_measurement(sd: &'static Softdevice) -> f32 {
    use rand::Rng as _;
    let mut rng = SoftDeviceRng(sd);

    static TIME: Lazy<usize> = Lazy::new(|| 0);
    Timer::after(SAMPLING_INTERVAL).await;
    rng.gen_range(10.0..20.0)
}
