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

use super::{Sample, SampleProducerMut};
use core::ops::Sub;

pub struct Tarer<T>
where
    T: SampleProducerMut,
{
    sampler: T,
    offset: T::Output,
}

impl<T> Tarer<T>
where
    T: SampleProducerMut,
{
    pub fn new(sampler: T) -> Self
    where
        T::Output: Default,
    {
        Self {
            sampler,
            offset: Default::default(),
        }
    }

    pub fn set_offset(&mut self, offset: T::Output) {
        self.offset = offset;
    }
}

impl<T> SampleProducerMut for Tarer<T>
where
    T: SampleProducerMut,
    T::Output: Sub<Output = T::Output>,
    T::Output: Copy + defmt::Format,
{
    type Output = T::Output;

    async fn sample(&mut self) -> Sample<Self::Output> {
        let mut sample = self.sampler.sample().await;
        sample.value = sample.value - self.offset;
        defmt::trace!("Tared = {}", sample.value);
        sample
    }
}
