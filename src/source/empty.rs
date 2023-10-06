use std::marker::PhantomData;
use std::time::Duration;

use crate::{Sample, Source};

use super::SeekNotSupported;

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
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> u16 {
        1
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        48000
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::new(0, 0))
    }

    #[inline]
    fn try_seek(&mut self, _: Duration) -> Result<(), SeekNotSupported> {
        Err(SeekNotSupported { source: std::any::type_name::<Self>() })
    }

    #[inline]
    fn can_seek(&self) -> bool {
        true
    }
}
