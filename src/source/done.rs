use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::Source;

/// When the inner source is empty this decrements a `AtomicUsize`.
/// Decrementing can be toggled using [`should_decrement`](Self::should_decrement).
#[derive(Debug, Clone)]
pub struct Done<I> {
    input: I,
    signal: Arc<AtomicUsize>,
    signal_sent: bool,
}

impl<I> Done<I> {
    /// When the inner source is empty the AtomicUsize passed in is decremented.
    /// If it was zero it will overflow negatively.
    #[inline]
    pub fn new(input: I, signal: Arc<AtomicUsize>) -> Done<I> {
        Done {
            input,
            signal,
            signal_sent: false,
        }
    }

    /// If this source didn't decrement the number already, calling this with
    /// `false` will prevent it from decrementing after the inner source ends.
    pub fn should_decrement(&mut self, should_decrement: bool) {
        self.signal_sent = !should_decrement;
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

impl<I: Source> Iterator for Done<I>
where
    I: Source,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        let next = self.input.next();
        if !self.signal_sent && next.is_none() {
            self.signal.fetch_sub(1, Ordering::Relaxed);
            self.signal_sent = true;
        }
        next
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> Source for Done<I>
where
    I: Source,
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
