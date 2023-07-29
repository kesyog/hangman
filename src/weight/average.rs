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

use core::ops::{AddAssign, Div, SubAssign};

pub(crate) trait Accumulator {
    type Sum;
}

impl Accumulator for f32 {
    type Sum = f64;
}

impl Accumulator for i32 {
    type Sum = i64;
}

// To reduce bloat, consider NOT using const generics if there are multiple instances
#[derive(Default)]
pub(crate) struct Window<const N: usize, T>
where
    T: Accumulator,
{
    accumulator: T::Sum,
    n_samples: usize,
    // TODO: delete min/max in window
    max: Option<T::Sum>,
    min: Option<T::Sum>,
}

impl<const N: usize, T> Window<N, T>
where
    T: Accumulator,
{
    pub fn add_sample(&mut self, sample: T) -> Option<T>
    where
        T::Sum: From<T>
            + SubAssign<T::Sum>
            + AddAssign<T::Sum>
            + Div<Output = T::Sum>
            + num::NumCast
            + Copy
            + PartialOrd,
        T: num::NumCast + Copy,
        Self: Default,
    {
        let sample: T::Sum = sample.into();
        self.accumulator += sample;
        self.n_samples += 1;

        match &mut self.max {
            Some(max) if *max >= sample => (),
            _ => self.max = Some(sample),
        }

        match &mut self.min {
            Some(min) if *min <= sample => (),
            _ => self.min = Some(sample),
        }

        if self.n_samples < N {
            return None;
        }

        // Remove max and min to reduce the impact of outliers iff window size is above an
        // arbitrary threshold
        if N > 5 {
            self.accumulator -= self.min.unwrap();
            self.accumulator -= self.max.unwrap();
            self.n_samples -= 2;
        }

        let average = self.accumulator / (<T::Sum as num::NumCast>::from(self.n_samples).unwrap());
        self.reset();
        Some(<T as num::NumCast>::from(average).unwrap())
    }

    pub fn reset(&mut self)
    where
        Self: Default,
    {
        *self = Self::default();
    }
}
