extern crate alloc;

use crate::nonvolatile::{Nvm, RegisterRead};
use crate::{WeightAdc, MEASURE_COMMAND_CHANNEL_SIZE};
use alloc::rc::Rc;
use core::num::NonZeroU32;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Receiver;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant, Timer};
use fix_hidden_lifetime_bug::fix_hidden_lifetime_bug;
use nrf_softdevice::Softdevice;
use once_cell::sync::Lazy;
use rand::RngCore;

// yellow CLK 0.20
// orange DATA 0.17
const SAMPLING_INTERVAL: Duration = Duration::from_hz(10);
pub const DEFAULT_CALIBRATION_M: f32 = 0.0000021950245;
pub const DEFAULT_CALIBRATION_B: i32 = 92554;

type ReceiveChannel = Receiver<'static, NoopRawMutex, Command, MEASURE_COMMAND_CHANNEL_SIZE>;
type ProtectedAdc = Mutex<NoopRawMutex, &'static mut WeightAdc>;
type OnMeasurementCb = dyn Fn(u32, f32);

pub enum Command {
    /// Start measuring continuously
    StartMeasurement(Rc<OnMeasurementCb>),
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
    Active(Rc<OnMeasurementCb>, Instant),
}

#[derive(Copy, Clone)]
struct Calibration {
    m: f32,
    b: i32,
}

impl Default for Calibration {
    fn default() -> Self {
        Self {
            m: DEFAULT_CALIBRATION_M,
            b: DEFAULT_CALIBRATION_B,
        }
    }
}

#[derive(Clone)]
struct MeasurementContext {
    state: MeasurementState,
    offset: f32,
    calibration: Calibration,
}

async fn handle_command(
    rx: &ReceiveChannel,
    context: &MeasurementContext,
    adc: &ProtectedAdc,
) -> MeasurementContext {
    let mut context = context.clone();
    let rx_cmd = rx.try_recv();
    if rx_cmd.is_ok() {
        defmt::debug!("Measure task received {}", rx_cmd);
    }
    match rx_cmd {
        Ok(Command::StartMeasurement(measurement_cb)) => {
            {
                let mut leds = crate::leds::singleton_get().lock().await;
                leds.rgb_red.set_low();
            }
            adc.lock().await.power_up();
            context.state = MeasurementState::Active(measurement_cb, Instant::now())
        }
        Ok(Command::StopMeasurement) => {
            {
                let mut leds = crate::leds::singleton_get().lock().await;
                leds.rgb_red.set_high();
            }
            adc.lock().await.power_down();
            context.state = MeasurementState::Idle
        }
        Ok(Command::Tare) => {
            let measurement = take_measurement(adc, &context.calibration, context.offset).await;
            context.offset -= measurement;
        }
        Err(_) => (),
    }
    context
}

// Workaround for Rust compiler bug
// See https://github.com/danielhenrymantilla/fix_hidden_lifetime_bug.rs
#[allow(clippy::manual_async_fn)]
#[fix_hidden_lifetime_bug]
async fn measure(context: &MeasurementContext, adc: &ProtectedAdc) {
    let MeasurementState::Active(ref measurement_cb, start_time) = context.state else {
        return;
    };
    let measurement = take_measurement(adc, &context.calibration, context.offset).await;
    let timestamp = Instant::now().duration_since(start_time).as_micros() as u32;
    measurement_cb(timestamp, measurement);
}

#[embassy_executor::task]
pub async fn measure_task(
    rx: ReceiveChannel,
    adc: &'static mut WeightAdc,
    sd: &'static Softdevice,
) {
    defmt::info!("Starting measurement task");
    let nvm = Nvm::new(sd);
    let calibration = Calibration {
        m: f32::from_le_bytes(nvm.read(RegisterRead::CalibrationM)),
        b: i32::from_le_bytes(nvm.read(RegisterRead::CalibrationB)),
    };
    defmt::info!("Calibration: m={} b={}", calibration.m, calibration.b);
    let mut context = MeasurementContext {
        state: MeasurementState::Idle,
        offset: 0.0,
        calibration,
    };
    let adc: Mutex<NoopRawMutex, &mut WeightAdc> = Mutex::new(adc);

    loop {
        context = handle_command(&rx, &context, &adc).await;
        if let MeasurementState::Active(..) = context.state {
            measure(&context, &adc).await;
        } else {
            // Sleep to give a chance for other tasks to run
            Timer::after(Duration::from_millis(100)).await;
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

async fn take_measurement(adc: &ProtectedAdc, calibration: &Calibration, offset: f32) -> f32 {
    let mut adc = adc.lock().await;
    if !adc.is_powered() {
        adc.power_up();
    }
    let reading = adc.take_measurement().await.unwrap();
    let adjusted = (reading + calibration.b) as f32 * calibration.m + offset;

    defmt::debug!(
        "raw={} offset={} calibration: ({} {}) adjusted={}",
        reading,
        offset,
        calibration.m,
        calibration.b,
        adjusted,
    );
    adjusted
}

#[allow(unused)]
async fn take_fake_measurement(sd: &'static Softdevice) -> f32 {
    use rand::Rng as _;
    let mut rng = SoftDeviceRng(sd);

    static TIME: Lazy<usize> = Lazy::new(|| 0);
    Timer::after(SAMPLING_INTERVAL).await;
    rng.gen_range(10.0..20.0)
}
