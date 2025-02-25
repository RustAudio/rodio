use std::time::Duration;

use crate::{ChannelCount, Sample, SampleRate};

use super::Source;

pub struct Peekable<I> {
    next: Option<Sample>,
    channels: ChannelCount,
    sample_rate: SampleRate,
    parameters_changed_after_next: bool,
    parameters_changed: bool,
    inner: I,
}

impl<I> Clone for Peekable<I>
where
    I: Clone,
{
    fn clone(&self) -> Self {
        Self {
            next: self.next,
            channels: self.channels,
            sample_rate: self.sample_rate,
            parameters_changed_after_next: self.parameters_changed_after_next,
            parameters_changed: self.parameters_changed,
            inner: self.inner.clone(),
        }
    }
}

impl<I: Source> Peekable<I> {
    pub(crate) fn new(mut inner: I) -> Peekable<I> {
        Self {
            // field order is critical! do not change
            channels: inner.channels(),
            sample_rate: inner.sample_rate(),
            next: inner.next(),
            parameters_changed_after_next: inner.parameters_changed(),
            parameters_changed: false,
            inner,
        }
    }

    pub fn peek(&self) -> Option<Sample> {
        self.next
    }
}

impl<I: Source> Iterator for Peekable<I> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.next.take()?;
        self.next = self.inner.next();
        self.parameters_changed = self.parameters_changed_after_next;
        self.parameters_changed_after_next = self.inner.parameters_changed();
        Some(item)
    }
}

impl<I: Source> Source for Peekable<I> {
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
