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

use embassy_nrf::gpio::{AnyPin, Input, Pull};

pub enum Polarity {
    ActiveLow,
    ActiveHigh,
}

pub struct Button {
    input: Input<'static, AnyPin>,
    polarity: Polarity,
}

impl Button {
    pub fn new(pin: AnyPin, polarity: Polarity, pull: bool) -> Self {
        // let mut button = gpio::Input::new(p.P1_06, gpio::Pull::Up);
        let input = match polarity {
            Polarity::ActiveLow => {
                let pull = if pull { Pull::Up } else { Pull::None };
                Input::new(pin, pull)
            }
            Polarity::ActiveHigh => {
                let pull = if pull { Pull::Down } else { Pull::None };
                Input::new(pin, pull)
            }
        };
        Self { input, polarity }
    }

    pub async fn wait_for_press(&mut self) {
        match self.polarity {
            Polarity::ActiveLow => self.input.wait_for_falling_edge().await,
            Polarity::ActiveHigh => self.input.wait_for_rising_edge().await,
        }
    }
}
