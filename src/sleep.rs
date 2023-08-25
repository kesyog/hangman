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

use crate::{button::Button, pac, util};

/// Set system into system OFF mode with the given button as the wakeup trigger.
///
/// Upon wakeup, the MCU will reset with no RAM retained. Some system registers may retain their
/// previous values (see nRF52 manual). The wakeup button's GPIO latch line should be reset at boot
/// to be able to work with the `embassy_nrf` HAL's GPIO event functionality.
///
/// # Safety
///
/// Should not be called with any pending GPIO events
pub async unsafe fn system_off(mut wakeup_button: Button) -> ! {
    // Set up wakeup button to trigger the SENSE signal on push, which wakes up the system from
    // system OFF
    // SAFETY:
    // * We have exclusive control of the button
    // * There are no pending GPIO events
    unsafe {
        util::disable_all_gpio_sense();
        wakeup_button.enable_sense();
        (*pac::P0::ptr()).latch.write(|w| w.bits(0xFFFFFFFF));
        (*pac::P1::ptr()).latch.write(|w| w.bits(0xFFFFFFFF));
    }

    defmt::info!("Going to system OFF");
    unsafe { nrf_softdevice::raw::sd_power_system_off() };
    defmt::info!("Good night, moon");

    // We need an infinite loop here for emulated system OFF mode (with a debugger attached)
    #[allow(clippy::empty_loop)]
    loop {}
}
