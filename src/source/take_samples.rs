use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::math::PrevMultipleOf;
use crate::Source;

/// A source that truncates the given source to a certain duration.
#[derive(Clone, Debug)]
pub struct TakeSamples<I> {
    input: I,
    taken: usize,
    target: usize,
}

impl<I> TakeSamples<I>
where
    I: Source,
{
    pub fn new(input: I, n: usize) -> Self {
        Self {
            input,
            taken: 0,
            target: n,
        }
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

impl<I> Iterator for TakeSamples<I>
where
    I: Source,
{
    type Item = <I as Iterator>::Item;

    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        if self.taken >= self.target.prev_multiple_of(self.input.channels()) {
            None
        } else {
            self.taken += 1;
            self.input.next()
        }
    }
}

// TODO: size_hint

impl<I> Source for TakeSamples<I>
where
    I: Iterator + Source,
{
    #[inline]
    fn parameters_changed(&self) -> bool {
        self.input.parameters_changed()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.input.channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.input.try_seek(pos)
    }
}
