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

use super::{Sample, SampleProducer, SampleProducerMut};
use typenum::U5;

pub(crate) struct Median<T>
where
    T: SampleProducer,
{
    source: T,
    accumulator: median::stack::Filter<T::Output, U5>,
}

impl<T> Median<T>
where
    T: SampleProducer,
    T::Output: Clone + PartialOrd,
{
    pub(crate) fn new(source: T) -> Self {
        Self {
            source,
            accumulator: median::stack::Filter::new(),
        }
    }
}

impl<T> SampleProducerMut for Median<T>
where
    T: SampleProducer,
    T::Output: Clone + PartialOrd,
{
    type Output = T::Output;

    async fn sample(&mut self) -> Sample<Self::Output> {
        let sample = self.source.sample().await;
        self.accumulator.consume(sample.value);
        Sample {
            timestamp: sample.timestamp,
            value: self.accumulator.median(),
        }
    }
}
