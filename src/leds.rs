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

use embassy_nrf::gpio::{AnyPin, Output};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;
use once_cell::sync::OnceCell;

static LEDS: OnceCell<Mutex<ThreadModeRawMutex, Leds<'static>>> = OnceCell::new();

pub fn singleton_init(
    rgb_blue: Output<'static, AnyPin>,
    rgb_red: Output<'static, AnyPin>,
    green: Output<'static, AnyPin>,
) -> Result<(), ()> {
    LEDS.set(Mutex::new(Leds::new(rgb_blue, rgb_red, green)))
        .map_err(|_| ())
}

pub fn singleton_get() -> &'static Mutex<ThreadModeRawMutex, Leds<'static>> {
    LEDS.get().expect("Leds to be initialized")
}

pub struct Leds<'d> {
    pub rgb_blue: Output<'d, AnyPin>,
    pub rgb_red: Output<'d, AnyPin>,
    pub green: Output<'d, AnyPin>,
}

impl<'d> Leds<'d> {
    fn new(
        rgb_blue: Output<'d, AnyPin>,
        rgb_red: Output<'d, AnyPin>,
        green: Output<'d, AnyPin>,
    ) -> Self {
        Self {
            rgb_blue,
            rgb_red,
            green,
        }
    }
}
