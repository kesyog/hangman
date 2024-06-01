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

use crate::pac;

pub unsafe fn disable_all_gpio_sense() {
    #[cfg(feature = "nrf52840")]
    {
        let p1 = unsafe { &(*pac::P1::ptr()) };
        for cnf in &p1.pin_cnf {
            cnf.modify(|_, w| w.sense().disabled());
        }
    }
    let p0 = unsafe { &(*pac::P0::ptr()) };
    for cnf in &p0.pin_cnf {
        cnf.modify(|_, w| w.sense().disabled());
    }
}
