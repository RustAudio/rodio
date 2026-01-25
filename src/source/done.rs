use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::Source;

/// When the inner source is exhausted, this source calls a callback once
/// with the mutable reference to the inner source.
#[derive(Debug, Clone)]
pub struct Done<I, F>
where
    F: FnMut(&mut I),
{
    input: I,
    callback: F,
    signal_sent: bool,
}

impl<I, F> Done<I, F>
where
    F: FnMut(&mut I),
{
    /// When the inner source is exhausted, this source calls a callback once
    /// with the mutable reference to the inner source.
    #[inline]
    pub fn new(input: I, callback: F) -> Done<I, F> {
        Done {
            input,
            callback,
            signal_sent: false,
        }
    }

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

impl<I, F> Iterator for Done<I, F>
where
    I: Source,
    F: FnMut(&mut I),
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        let next = self.input.next();
        if !self.signal_sent && next.is_none() {
            self.signal_sent = true;
            (self.callback)(&mut self.input);
        }
        next
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I, F> ExactSizeIterator for Done<I, F>
where
    I: Source + ExactSizeIterator,
    F: FnMut(&mut I),
{
}

impl<I, F> Source for Done<I, F>
where
    I: Source,
    F: FnMut(&mut I),
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
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
