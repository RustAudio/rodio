use std::marker::PhantomData;
use std::time::Duration;

use Sample;
use Source;

/// An iterator that reads from a `Source` and converts the samples to a specific rate and
/// channels count.
///
/// It implements `Source` as well, but all the data is guaranteed to be in a single frame whose
/// channels and samples rate have been passed to `new`.
#[derive(Clone)]
pub struct SamplesConverter<I, D>
    where I: Source,
          I::Item: Sample,
          D: Sample
{
    inner: I,
    dest: PhantomData<D>,
}

impl<I, D> SamplesConverter<I, D>
    where I: Source,
          I::Item: Sample,
          D: Sample
{
    #[inline]
    pub fn new(input: I) -> SamplesConverter<I, D> {
        SamplesConverter {
            inner: input,
            dest: PhantomData,
        }
    }
}

impl<I, D> Iterator for SamplesConverter<I, D>
    where I: Source,
          I::Item: Sample,
          D: Sample
{
    type Item = D;

    #[inline]
    fn next(&mut self) -> Option<D> {
        self.inner.next().map(|s| Sample::from(&s))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<I, D> ExactSizeIterator for SamplesConverter<I, D>
    where I: Source + ExactSizeIterator,
          I::Item: Sample,
          D: Sample
{
}

impl<I, D> Source for SamplesConverter<I, D>
    where I: Source,
          I::Item: Sample,
          D: Sample
{
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        self.inner.get_current_frame_len()
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        self.inner.get_channels()
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        self.inner.get_samples_rate()
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        self.inner.get_total_duration()
    }
}
