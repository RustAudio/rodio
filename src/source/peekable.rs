use std::time::Duration;

use crate::{ChannelCount, Sample, SampleRate};

use super::Source;

/// A source with a `peek()` method that returns the next sample without
/// advancing the source. This `struct` is created by the
/// [`peekable_source`](Source::peekable_source) method on [Source]. See its
/// documentation for more.
pub struct PeekableSource<I> {
    next: Option<Sample>,
    channels: ChannelCount,
    sample_rate: SampleRate,
    channels_after_next: ChannelCount,
    sample_rate_after_next: SampleRate,
    parameters_changed_after_next: bool,
    parameters_changed: bool,
    inner: I,
}

impl<I: Clone> Clone for PeekableSource<I> {
    fn clone(&self) -> Self {
        Self {
            next: self.next,
            channels: self.channels,
            sample_rate: self.sample_rate,
            parameters_changed_after_next: self.parameters_changed_after_next,
            parameters_changed: self.parameters_changed,
            inner: self.inner.clone(),
            channels_after_next: self.channels_after_next,
            sample_rate_after_next: self.sample_rate_after_next,
        }
    }
}

impl<I: Source> PeekableSource<I> {
    pub(crate) fn new(mut inner: I) -> PeekableSource<I> {
        Self {
            // field order is critical! do not change
            channels: inner.channels(),
            sample_rate: inner.sample_rate(),
            next: inner.next(),
            channels_after_next: inner.channels(),
            sample_rate_after_next: inner.sample_rate(),
            parameters_changed_after_next: inner.parameters_changed(),
            parameters_changed: false,
            inner,
        }
    }

    /// Look at the next sample. This does not advance the source.
    /// Can be used to determine if the current sample was the last.
    pub fn peek_next(&self) -> Option<Sample> {
        self.next
    }

    /// Do the parameters change after the next sample? This does not advance 
    /// the source.
    pub fn peek_parameters_changed(&self) -> bool {
        self.parameters_changed_after_next
    }
}

impl<I: Source> Iterator for PeekableSource<I> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.next.take()?;
        self.next = self.inner.next();

        self.parameters_changed = self.parameters_changed_after_next;
        self.channels = self.channels_after_next;
        self.sample_rate = self.sample_rate_after_next;

        self.parameters_changed_after_next = self.inner.parameters_changed();
        self.channels_after_next = self.inner.channels();
        self.sample_rate_after_next = self.inner.sample_rate();

        Some(item)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<I: ExactSizeIterator + Source> ExactSizeIterator for PeekableSource<I> {}

impl<I: Source> Source for PeekableSource<I> {
    fn parameters_changed(&self) -> bool {
        self.parameters_changed
    }

    fn channels(&self) -> ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}
