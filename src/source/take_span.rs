use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::Source;

/// A source that truncates the given source to a certain duration.
#[derive(Clone, Debug)]
pub struct TakeSpan<I> {
    input: I,
}

impl<I> TakeSpan<I>
where
    I: Source,
{
    pub fn new(input: I) -> Self {
        Self { input }
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

impl<I> Iterator for TakeSpan<I>
where
    I: Source,
{
    type Item = <I as Iterator>::Item;

    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        if self.input.parameters_changed() {
            None
        } else {
            let sample = self.input.next()?;
            Some(sample)
        }
    }
}

// TODO: size_hint

impl<I> Source for TakeSpan<I>
where
    I: Iterator + Source,
{
    #[inline]
    fn parameters_changed(&self) -> bool {
        false
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
