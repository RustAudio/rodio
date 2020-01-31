use std::time::Duration;

use crate::Sample;
use crate::Source;

/// Internal function that builds a `Speed` object.
pub fn speed<I>(input: I, factor: f32) -> Speed<I> {
    Speed {
        input: input,
        factor: factor,
    }
}

/// Filter that modifies each sample by a given value.
#[derive(Clone, Debug)]
pub struct Speed<I> {
    input: I,
    factor: f32,
}

impl<I> Speed<I>
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

impl<I> Iterator for Speed<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        self.input.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for Speed<I>
where
    I: Source + ExactSizeIterator,
    I::Item: Sample,
{
}

impl<I> Source for Speed<I>
where
    I: Source,
    I::Item: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        self.input.current_frame_len()
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.input.channels()
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        (self.input.sample_rate() as f32 * self.factor) as u32
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        // TODO: the crappy API of duration makes this code difficult to write
        if let Some(duration) = self.input.total_duration() {
            let as_ns = duration.as_secs() * 1000000000 + duration.subsec_nanos() as u64;
            let new_val = (as_ns as f32 / self.factor) as u64;
            Some(Duration::new(
                new_val / 1000000000,
                (new_val % 1000000000) as u32,
            ))
        } else {
            None
        }
    }
}
