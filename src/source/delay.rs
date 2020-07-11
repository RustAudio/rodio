use std::time::Duration;

use crate::Sample;
use crate::Source;

/// Internal function that builds a `Delay` object.
pub fn delay<I>(input: I, duration: Duration) -> Delay<I>
where
    I: Source,
    I::Item: Sample,
{
    let duration_ns = duration.as_secs() * 1000000000 + duration.subsec_nanos() as u64;
    let samples = duration_ns * input.sample_rate() as u64 / 1000000000 * input.channels() as u64;

    Delay {
        input: input,
        remaining_samples: samples as usize,
        requested_duration: duration,
    }
}

/// A source that delays the given source by a certain amount.
#[derive(Clone, Debug)]
pub struct Delay<I> {
    input: I,
    remaining_samples: usize,
    requested_duration: Duration,
}

impl<I> Delay<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Returns a reference to the inner source.
    #[inline]
    pub fn inner(&self) -> &I {
        &self.input
    }

    /// Returns a mutable reference to the inner source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.input
    }

    /// Returns the inner source.
    #[inline]
    pub fn into_inner(self) -> I {
        self.input
    }
}

impl<I> Iterator for Delay<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = <I as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        if self.remaining_samples >= 1 {
            self.remaining_samples -= 1;
            Some(Sample::zero_value())
        } else {
            self.input.next()
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (min, max) = self.input.size_hint();
        (
            min + self.remaining_samples,
            max.map(|v| v + self.remaining_samples),
        )
    }
}

impl<I> Source for Delay<I>
where
    I: Iterator + Source,
    I::Item: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        self.input
            .current_frame_len()
            .map(|val| val + self.remaining_samples)
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.input.channels()
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input
            .total_duration()
            .map(|val| val + self.requested_duration)
    }
}
