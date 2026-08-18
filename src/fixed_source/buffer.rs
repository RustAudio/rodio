use std::sync::Arc;
use std::time::Duration;

use crate::source::SeekError;
use crate::FixedSource;
use crate::{ChannelCount, Sample, SampleRate};

/// A buffer of samples treated as a source.
#[derive(Debug, Clone)]
pub struct SamplesBuffer {
    data: Arc<[Sample]>,
    pos: usize,
    channels: ChannelCount,
    sample_rate: SampleRate,
}

impl SamplesBuffer {
    /// Builds a new `SamplesBuffer`.
    pub fn new<D>(channels: ChannelCount, sample_rate: SampleRate, data: D) -> SamplesBuffer
    where
        D: Into<Vec<Sample>>,
    {
        let data: Arc<[Sample]> = data.into().into();
        SamplesBuffer {
            data,
            pos: 0,
            channels,
            sample_rate,
        }
    }
}

impl FixedSource for SamplesBuffer {
    #[inline]
    fn channels(&self) -> ChannelCount {
        self.channels
    }
    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }
    crate::common::source::buffer::source_impl! {}
}

impl Iterator for SamplesBuffer {
    crate::common::source::buffer::iter_impl! {}
}
