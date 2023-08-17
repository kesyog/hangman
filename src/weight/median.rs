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
