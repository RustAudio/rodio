use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::{Sample, Source};
use dasp_sample::{Sample as DaspSample, ToSample};

// TODO Can we reuse conversions::SampleTypeConverter here? This file declares a
//      converting source while conversions::SampleTypeConverter is an iterator.

// FIXME (implementation, #678) Clarify how the input should now be provided
//       (need a non f32 source or an iterator still).

/// Wrap the input and lazily converts the samples it provides to the type
/// specified by the generic parameter D
#[derive(Clone)]
pub struct SamplesConverter<I> {
    inner: I,
}

impl<I> SamplesConverter<I>
where
    I: Iterator,
    I::Item: ToSample<Sample>,
{
    /// Wrap the input and lazily converts the samples it provides to the type
    /// specified by the generic parameter D
    #[inline]
    pub fn new(input: I) -> SamplesConverter<I> {
        SamplesConverter { inner: input }
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

impl<I> Iterator for SamplesConverter<I>
where
    I: Iterator,
    I::Item: ToSample<Sample> + DaspSample,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|s| DaspSample::to_sample(s))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<I> ExactSizeIterator for SamplesConverter<I>
where
    I: Iterator + ExactSizeIterator,
    I::Item: ToSample<Sample> + DaspSample,
{
}

impl<I> Source for SamplesConverter<I>
where
    I: Iterator,
    I::Item: ToSample<Sample> + DaspSample,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        todo!();
        // self.inner.current_span_len()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        todo!();
        // self.inner.channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        todo!();
        // self.inner.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        todo!();
        // self.inner.total_duration()
    }

    #[inline]
    fn try_seek(&mut self, _pos: Duration) -> Result<(), SeekError> {
        todo!();
        // self.inner.try_seek(pos)
    }
}
