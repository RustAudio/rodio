use std::marker::PhantomData;
use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::{Sample, Source};

/// An empty source.
#[derive(Debug, Copy, Clone)]
pub struct Empty<S>(PhantomData<S>);

impl<S> Default for Empty<S> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Empty<S> {
    /// An empty source that immediately ends without ever returning a sample to
    /// play
    #[inline]
    pub fn new() -> Empty<S> {
        Empty(PhantomData)
    }
}

impl<S> Iterator for Empty<S> {
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        None
    }
}

impl<S> Source for Empty<S>
where
    S: Sample,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        1
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        48000
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
