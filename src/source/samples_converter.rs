use std::marker::PhantomData;
use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::{Sample, Source};
use dasp_sample::{FromSample, Sample as DaspSample};

/// Wrap the input and lazily converts the samples it provides to the type
/// specified by the generic parameter D
#[derive(Clone)]
pub struct SamplesConverter<I, D> {
    inner: I,
    dest: PhantomData<D>,
}

impl<I, D> SamplesConverter<I, D> {
    /// Wrap the input and lazily converts the samples it provides to the type
    /// specified by the generic parameter D
    #[inline]
    pub fn new(input: I) -> SamplesConverter<I, D> {
        SamplesConverter {
            inner: input,
            dest: PhantomData,
        }
    }

    /// Returns a reference to the inner source.
    #[inline]
    pub fn inner(&self) -> &I {
        &self.inner
    }

    /// Returns a mutable reference to the inner source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.inner
    }

    /// Returns the inner source.
    #[inline]
    pub fn into_inner(self) -> I {
        self.inner
    }
}

impl<I, D> Iterator for SamplesConverter<I, D>
where
    I: Source,
    I::Item: Sample,
    D: FromSample<I::Item> + Sample,
{
    type Item = D;

    #[inline]
    fn next(&mut self) -> Option<D> {
        self.inner.next().map(|s| DaspSample::from_sample(s))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<I, D> ExactSizeIterator for SamplesConverter<I, D>
where
    I: Source + ExactSizeIterator,
    I::Item: Sample,
    D: FromSample<I::Item> + Sample,
{
}

impl<I, D> Source for SamplesConverter<I, D>
where
    I: Source,
    I::Item: Sample,
    D: FromSample<I::Item> + Sample,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)
    }
}
