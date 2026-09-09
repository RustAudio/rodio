use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::Source;

/// This is the same as [`skippable`](crate::source::skippable) see its docs
pub fn stoppable<I>(source: I) -> Stoppable<I> {
    Stoppable {
        input: source,
        stopped: false,
    }
}

/// This is the same as [`Skippable`](crate::source::Skippable) see its docs
#[derive(Clone, Debug)]
pub struct Stoppable<I> {
    input: I,
    stopped: bool,
}

impl<S> Stoppable<S> {
    /// Stops the sound.
    #[inline]
    pub fn stop(&mut self) {
        self.stopped = true;
    }

    crate::common::source::add_inner_accessors! {input}
}

impl<I> Iterator for Stoppable<I>
where
    I: Source,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        if self.stopped {
            None
        } else {
            self.input.next()
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for Stoppable<I> where I: Source + ExactSizeIterator {}

impl<I> Source for Stoppable<I>
where
    I: Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        if self.stopped {
            Some(0)
        } else {
            self.input.current_span_len()
        }
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
