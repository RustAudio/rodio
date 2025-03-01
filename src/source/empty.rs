use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::math::nz;
use crate::{Sample, Source};

/// An empty source.
#[derive(Debug, Copy, Clone)]
pub struct Empty();

impl Default for Empty {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Empty {
    /// An empty source that immediately ends without ever returning a sample to
    /// play
    #[inline]
    pub fn new() -> Empty {
        Empty()
    }
}

impl Iterator for Empty {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}

impl Source for Empty {
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        nz!(1)
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        nz!(48000)
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::new(0, 0))
    }

    #[inline]
    fn try_seek(&mut self, _: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported {
            underlying_source: std::any::type_name::<Self>(),
        })
    }
}
