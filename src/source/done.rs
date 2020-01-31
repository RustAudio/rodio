use crate::Sample;
use crate::Source;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// When the inner source is empty this decrements an `AtomicUsize`.
#[derive(Debug, Clone)]
pub struct Done<I> {
    input: I,
    signal: Arc<AtomicUsize>,
    signal_sent: bool,
}

impl<I> Done<I> {
    #[inline]
    pub fn new(input: I, signal: Arc<AtomicUsize>) -> Done<I> {
        Done {
            input,
            signal,
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

impl<I: Source> Iterator for Done<I>
where
    I: Source,
    I::Item: Sample,
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
}

impl<I> Source for Done<I>
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
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
}
