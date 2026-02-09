use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::{Sample, Source};

/// Creates a `Source` from a sample iterator with specified audio parameters.
///
/// This adapter wraps any iterator that produces `Sample` values and provides
/// the `Source` trait implementation by storing the channel count and sample rate.
///
/// # Example
///
/// ```
/// use rodio::source::from_iter;
/// use rodio::math::nz;
///
/// let samples = vec![0.1, 0.2, 0.3, 0.4];
/// let source = from_iter(samples.into_iter(), nz!(2), nz!(44100));
/// ```
#[inline]
pub fn from_iter<I>(iter: I, channels: ChannelCount, sample_rate: SampleRate) -> FromIter<I>
where
    I: Iterator<Item = Sample>,
{
    FromIter {
        iter,
        channels,
        sample_rate,
    }
}

/// A `Source` that wraps a sample iterator with audio metadata.
///
/// Created by the `from_iter()` function.
#[derive(Clone, Debug)]
pub struct FromIter<I> {
    iter: I,
    channels: ChannelCount,
    sample_rate: SampleRate,
}

impl<I> FromIter<I> {
    /// Creates a new `FromIter` from an iterator and audio parameters.
    #[inline]
    pub fn new(iter: I, channels: ChannelCount, sample_rate: SampleRate) -> Self {
        Self {
            iter,
            channels,
            sample_rate,
        }
    }

    /// Destroys this source and returns the underlying iterator.
    #[inline]
    pub fn into_inner(self) -> I {
        self.iter
    }

    /// Get immutable access to the underlying iterator.
    #[inline]
    pub fn inner(&self) -> &I {
        &self.iter
    }

    /// Get mutable access to the underlying iterator.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.iter
    }
}

impl<I> Iterator for FromIter<I>
where
    I: Iterator<Item = Sample>,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<I> Source for FromIter<I>
where
    I: Iterator<Item = Sample>,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.iter.size_hint().1
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.channels
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    #[inline]
    fn try_seek(&mut self, _pos: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported {
            underlying_source: std::any::type_name::<Self>(),
        })
    }
}

impl<I> ExactSizeIterator for FromIter<I> where I: ExactSizeIterator<Item = Sample> {}
