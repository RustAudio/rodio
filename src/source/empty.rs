use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::math::nz;
use crate::{Sample, Source};

/// An empty source.
#[derive(Debug, Copy, Clone)]
pub struct Empty {
    channels: ChannelCount,
    sample_rate: SampleRate,
}

impl Default for Empty {
    #[inline]
    fn default() -> Self {
        Self {
            channels: nz!(1),
            sample_rate: crate::DEFAULT_SAMPLE_RATE,
        }
    }
}

impl Empty {
    /// An empty source that immediately ends without ever returning a sample to
    /// play
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Like [`Empty::new`], but reports `channels`/`sample_rate` instead of a default format.
    /// Useful as a placeholder that won't need format conversion once given real content.
    #[inline]
    pub fn new_with_format(channels: ChannelCount, sample_rate: SampleRate) -> Self {
        Self {
            channels,
            sample_rate,
        }
    }
}

impl Iterator for Empty {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }
}

impl ExactSizeIterator for Empty {}

impl Source for Empty {
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        Some(0)
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
        Some(Duration::ZERO)
    }

    #[inline]
    fn try_seek(&mut self, _: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported {
            underlying_source: std::any::type_name::<Self>(),
        })
    }
}
