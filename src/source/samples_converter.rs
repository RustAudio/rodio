use std::marker::PhantomData;
use std::time::Duration;

use crate::Sample;
use crate::Source;
use cpal::Sample as CpalSample;

/// An iterator that reads from a `Source` and converts the samples to a specific rate and
/// channels count.
///
/// It implements `Source` as well, but all the data is guaranteed to be in a single frame whose
/// channels and samples rate have been passed to `new`.
#[derive(Clone)]
pub struct SamplesConverter<I, D> {
    inner: I,
    dest: PhantomData<D>,
}

impl<I, D> SamplesConverter<I, D> {
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
    D: Sample,
{
    type Item = D;

    #[inline]
    fn next(&mut self) -> Option<D> {
        self.inner.next().map(|s| CpalSample::from(&s))
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
    D: Sample,
{
}

impl<I, D> Source for SamplesConverter<I, D>
where
    I: Source,
    I::Item: Sample,
    D: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}
