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

use crate::pac::{
    self,
    p0::{RegisterBlock, PIN_CNF},
};
use embassy_nrf::gpio::{AnyPin, Input, Pin, Port, Pull};

pub enum Polarity {
    ActiveLow,
    ActiveHigh,
}

pub struct Button {
    input: Input<'static, AnyPin>,
    polarity: Polarity,
    port: Port,
    pin_number: u8,
}

impl Button {
    pub fn new(pin: AnyPin, polarity: Polarity, pull: bool) -> Self {
        let port = pin.port();
        let pin_number = pin.pin();
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
        Self {
            input,
            polarity,
            port,
            pin_number,
        }
    }

    pub async fn wait_for_press(&mut self) {
        match self.polarity {
            Polarity::ActiveLow => self.input.wait_for_falling_edge().await,
            Polarity::ActiveHigh => self.input.wait_for_rising_edge().await,
        }
    }

    unsafe fn steal_port(&mut self) -> &'static RegisterBlock {
        match self.port {
            Port::Port0 => unsafe { &(*pac::P0::ptr()) },
            #[cfg(feature = "nrf52840")]
            Port::Port1 => unsafe { &(*pac::P1::ptr()) },
        }
    }

    unsafe fn steal_pin_cnf(&mut self) -> &'static PIN_CNF {
        let port = unsafe { self.steal_port() };
        &port.pin_cnf[usize::from(self.pin_number)]
    }

    pub unsafe fn enable_sense(&mut self) {
        let cfg = unsafe { self.steal_pin_cnf() };
        match self.polarity {
            Polarity::ActiveLow => cfg.modify(|_, w| w.sense().low()),
            Polarity::ActiveHigh => cfg.modify(|_, w| w.sense().high()),
        }
    }
}
