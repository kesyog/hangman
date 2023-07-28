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

#[derive(Default, Clone, Copy)]
pub struct WindowAveragerInt<const N: usize> {
    accumulator: i64,
    n_samples: usize,
}

impl<const N: usize> WindowAveragerInt<N> {
    pub fn add_sample(&mut self, sample: i32) -> Option<i32> {
        self.accumulator += i64::from(sample);
        self.n_samples += 1;

        if self.n_samples >= N {
            let average = self.accumulator / (self.n_samples as i64);
            self.reset();
            Some(average as i32)
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Default, Clone, Copy)]
pub struct WindowAveragerFloat<const N: usize> {
    accumulator: f64,
    n_samples: usize,
}

impl<const N: usize> WindowAveragerFloat<N> {
    pub fn add_sample(&mut self, sample: f32) -> Option<f32> {
        self.accumulator += f64::from(sample);
        self.n_samples += 1;

        if self.n_samples >= N {
            let average = self.accumulator / (self.n_samples as f64);
            self.reset();
            Some(average as f32)
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
