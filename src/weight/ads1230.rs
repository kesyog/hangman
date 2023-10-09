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

/// Ads1230 driver using embassy_nrf-friendly types
use super::{Sample, SampleProducerMut};
use crate::{blocking_hal::prelude::_embedded_hal_blocking_delay_DelayUs, util, SharedDelay};
use embassy_nrf::gpio::{AnyPin, Input, Output};
use embassy_time::Instant;
use embassy_time::{Duration, Timer};

enum PowerState {
    Off,
    On,
}

enum Followup {
    None,
    /// Run offset calibration after measurement
    OffsetCalibration,
    /// Run offset calibration after wakeup from standby
    StandbyAndOffsetCalibration,
}

pub struct Ads1230<'d> {
    data: Input<'d, AnyPin>,
    clock: Output<'d, AnyPin>,
    vdda_on: Output<'d, AnyPin>,
    state: PowerState,
    delay: &'static SharedDelay,
}

impl<'d> Ads1230<'d> {
    pub fn new(
        data: Input<'d, AnyPin>,
        mut clock: Output<'d, AnyPin>,
        vdda_on: Output<'d, AnyPin>,
        delay: &'static SharedDelay,
    ) -> Self {
        clock.set_high();
        Self {
            data,
            clock,
            vdda_on,
            state: PowerState::Off,
            delay,
        }
    }

    fn is_powered(&self) -> bool {
        matches!(self.state, PowerState::On)
    }

    pub fn power_down(&mut self) {
        self.clock.set_high();
        self.vdda_on.set_high();
        self.state = PowerState::Off;
    }

    pub async fn power_up(&mut self) {
        self.clock.set_low();
        self.vdda_on.set_low();
        // Give plenty of time (relative to Proto1.0 RC time constants) for the analog supply
        // voltage to settle
        Timer::after(Duration::from_micros(100)).await;
        self.state = PowerState::On;
    }

    async fn take_measurement(&mut self, action: Followup) -> Option<Sample<i32>> {
        if let PowerState::Off = self.state {
            return None;
        }

        let mut n_skips: usize = 0;

        loop {
            self.data.wait_for_low().await;
            let timestamp = Instant::now();
            let mut delay = self.delay.lock().await;

            // Use a critical section to minimize the chance of interrupts causing unexpected delays
            // We're still at the mercy of the Softdevice, but there's no escaping that
            let raw_reading = critical_section::with(|_| {
                let mut reading = 0;
                for i in (0..20).rev() {
                    self.clock.set_high();
                    delay.delay_us(1_u8);
                    if self.data.is_high() {
                        reading |= 1 << i;
                    }
                    delay.delay_us(1_u8);
                    self.clock.set_low();
                    delay.delay_us(1_u8);
                }

                // Additional pulses
                // 1 => force data back high
                // 6? => offset calibration
                let n_followup_pulses = match action {
                    Followup::None => 1,
                    Followup::OffsetCalibration => 6,
                    Followup::StandbyAndOffsetCalibration => 5,
                };
                for _ in 0..n_followup_pulses {
                    self.clock.set_high();
                    delay.delay_us(1_u8);
                    self.clock.set_low();
                    delay.delay_us(1_u8);
                }
                if let Followup::StandbyAndOffsetCalibration = action {
                    self.power_down();
                }
                reading
            });

            // The ADS1230 gives a 20-bit signed reading, which is initially stored in a u32 container.
            // Unsigned for sane shifting and 32-bit because there is no u20 Rust primitive. Convert it
            // to a signed integer so that it is interpreted correctly.
            let value = util::convert_signed_to_i32::<20>(raw_reading);
            // HX711 sometimes spontaneously returns -1 (0xFFFFFF)
            if value == -1 && n_skips < 3 {
                n_skips += 1;
                defmt::info!("Skipping -1 reading");
            } else {
                defmt::trace!("Raw = 0x{=u32:X}", raw_reading);
                return Some(Sample { timestamp, value });
            }
        }
    }

    pub async fn immediate_offset_calibration(&mut self) -> Option<Sample<i32>> {
        self.take_measurement(Followup::OffsetCalibration).await
    }

    pub async fn schedule_offset_calibration(&mut self) {
        self.take_measurement(Followup::StandbyAndOffsetCalibration)
            .await;
    }
}

impl<'d> SampleProducerMut for Ads1230<'d> {
    type Output = i32;

    async fn sample(&mut self) -> Sample<<Ads1230<'d> as SampleProducerMut>::Output> {
        if !self.is_powered() {
            self.power_up().await;
        }
        self.take_measurement(Followup::None).await.unwrap()
    }
}

impl<'d> SampleProducerMut for &mut Ads1230<'d> {
    type Output = i32;

    async fn sample(&mut self) -> Sample<<Ads1230<'d> as SampleProducerMut>::Output> {
        if !self.is_powered() {
            self.power_up().await;
        }
        self.take_measurement(Followup::None).await.unwrap()
    }
}
