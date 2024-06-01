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

//! Battery voltage sampler
//!
//! To ensure that the ADC , this module only

use embassy_nrf::interrupt::typelevel::Binding;
use embassy_nrf::peripherals::SAADC;
use embassy_nrf::saadc::{
    ChannelConfig, Config, Gain, InterruptHandler, Oversample, Reference, Resistor, Resolution,
    Saadc, VddInput,
};
use embassy_nrf::Peripheral;
use once_cell::sync::OnceCell;

static BATTERY_VOLTAGE: OnceCell<u32> = OnceCell::new();
/// Constant to multiply raw ADC readings by to get a voltage reading
// ADC reference = 0.6V
// Gain = 1/6
// Resolution = 14-bit
const ADC_SCALE_FACTOR: f32 = 0.6 * 6.0 / (2u32.pow(14) as f32);
/// Threshold for shutdown with warning
// This is just CRITICAL_BATTERY_THRESHOLD_MV plus some hand wavy margin
const LOW_BATTERY_THRESHOLD_MV: u32 = 2750;
/// Threshold for immediate shutdown
// ADS1230 minimum supply voltage is 2.7V
const CRITICAL_BATTERY_THRESHOLD_MV: u32 = 2700;

/// Samples battery voltage
///
/// Should only be called once
pub async fn one_time_sample(
    adc_peripheral: impl Peripheral<P = SAADC>,
    irqs: impl Binding<embassy_nrf::interrupt::typelevel::SAADC, InterruptHandler>,
) -> u32 {
    let mut channel_config = ChannelConfig::single_ended(VddInput);
    channel_config.gain = Gain::GAIN1_6;
    channel_config.resistor = Resistor::BYPASS;
    // 0.6V internal reference
    channel_config.reference = Reference::INTERNAL;

    let mut config = Config::default();
    config.resolution = Resolution::_14BIT;
    config.oversample = Oversample::OVER256X;

    let mut adc = Saadc::new(adc_peripheral, irqs, config, [channel_config]);

    adc.calibrate().await;
    let reading = sample(&mut adc).await;
    BATTERY_VOLTAGE
        .set(reading)
        .expect("one_time_sample to be called at most once");
    reading
}

pub fn get_startup_reading() -> Option<u32> {
    BATTERY_VOLTAGE.get().copied()
}

async fn sample<'a>(adc: &mut Saadc<'a, 1>) -> u32 {
    let mut buffer = [0i16];
    adc.sample(&mut buffer).await;
    let battery_voltage = buffer[0] as f32 * ADC_SCALE_FACTOR;
    (battery_voltage * 1000.0) as u32
}

pub fn is_low() -> bool {
    get_startup_reading().expect("Battery to be sampled") <= LOW_BATTERY_THRESHOLD_MV
}

pub fn is_critically_low() -> bool {
    get_startup_reading().expect("Battery to be sampled") <= CRITICAL_BATTERY_THRESHOLD_MV
}
