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

/// Hx711 driver using embassy_nrf-friendly types
///
/// TODO: can we replace with https://docs.rs/hx711 since we're using mostly operations anyway
use crate::SharedDelay;
use embassy_nrf::gpio::{Input, Output, Pin};
use nrf52840_hal::prelude::_embedded_hal_blocking_delay_DelayUs;

enum PowerState {
    Off,
    On,
}

pub struct Hx711<'d, I: Pin, O: Pin> {
    data: Input<'d, I>,
    clock: Output<'d, O>,
    state: PowerState,
    delay: &'static SharedDelay,
}

impl<'d, I: Pin, O: Pin> Hx711<'d, I, O> {
    pub fn new(data: Input<'d, I>, mut clock: Output<'d, O>, delay: &'static SharedDelay) -> Self {
        clock.set_high();
        Self {
            data,
            clock,
            state: PowerState::Off,
            delay,
        }
    }

    pub fn is_powered(&self) -> bool {
        matches!(self.state, PowerState::On)
    }

    pub fn power_down(&mut self) {
        self.clock.set_high();
        self.state = PowerState::Off;
    }

    pub fn power_up(&mut self) {
        self.clock.set_low();
        self.state = PowerState::On;
    }

    pub async fn take_measurement(&mut self) -> Option<i32> {
        if let PowerState::Off = self.state {
            return None;
        }

        self.data.wait_for_low().await;
        let mut delay = self.delay.lock().await;

        // Use a critical section to minimize the chance of interrupts causing unexpected delays
        // We're still at the mercy of the Softdevice, but there's no excaping that
        let raw_reading = critical_section::with(|_| {
            let mut reading = 0;
            for i in (0..24).rev() {
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
            // 1 => (CH1) gain = 128
            // 2 => (CH2) gain = 32 (not connected)
            // 3 => (CH1) gain = 64
            for _ in 0..1 {
                self.clock.set_high();
                delay.delay_us(1_u8);
                self.clock.set_low();
                delay.delay_us(1_u8);
            }
            reading
        });

        // The HX711 gives a 24-bit signed reading, which is initially stored in a u32 container.
        // Unsigned for sane shifting and 32-bit because there is no u24 Rust primitive. Convert it
        // to a signed integer so that it is interpreted correctly.
        let reading = convert_i24_to_i32(raw_reading);
        Some(reading)
    }
}

/// Convert a signed 24-bit integer in a u32 container to a signed integer
fn convert_i24_to_i32(mut input: u32) -> i32 {
    // Extend sign bits if negative
    if input & (1 << 23) != 0 {
        input |= 0xFF000000;
    }
    input as i32
}

// TODO: figure out how to actually run these tests on host
// I promise I ran them in the playground.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_conversion() {
        assert_eq!(convert_i24_to_i32(0x000000), 0);
        assert_eq!(convert_i24_to_i32(0x000001), 1);
        assert_eq!(convert_i24_to_i32(0x000002), 2);
        assert_eq!(convert_i24_to_i32(0x7FFFFE), 8388606);
        assert_eq!(convert_i24_to_i32(0x7FFFFF), 8388607);
        assert_eq!(convert_i24_to_i32(0x800000), -8388608);
        assert_eq!(convert_i24_to_i32(0x800001), -8388607);
        assert_eq!(convert_i24_to_i32(0xFFFFFF), -1);
        assert_eq!(convert_i24_to_i32(0xFFFFFE), -2);
    }
}
