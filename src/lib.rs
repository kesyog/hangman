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

#![cfg_attr(not(test), no_std)]
#![feature(type_alias_impl_trait)]
#![feature(async_fn_in_trait)]

#[cfg(feature = "console")]
pub mod console;
pub mod gatt;
pub mod leds;
pub mod nonvolatile;
pub mod weight;

use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    channel::{Channel, Receiver},
    mutex::Mutex,
};
use nrf52840_hal::Delay as SysTickDelay;
use nrf_softdevice as _;
use panic_probe as _;

pub type SharedDelay = Mutex<NoopRawMutex, SysTickDelay>;
pub type MeasureCommandChannel =
    Channel<NoopRawMutex, weight::Command, MEASURE_COMMAND_CHANNEL_SIZE>;
pub type MeasureCommandReceiver =
    Receiver<'static, NoopRawMutex, weight::Command, MEASURE_COMMAND_CHANNEL_SIZE>;

// Leave some room for multiple commands to be queued. If this is too small, we can get overwhelmed
// and deadlock.
pub const MEASURE_COMMAND_CHANNEL_SIZE: usize = 5;
