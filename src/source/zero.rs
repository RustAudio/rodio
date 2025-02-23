use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::{Sample, Source};

/// An source that produces samples with value zero (silence). Depending on if
/// it where created with [`Zero::new`] or [`Zero::new_samples`] it can be never
/// ending or finite.
#[derive(Clone, Debug)]
pub struct Zero {
    channels: ChannelCount,
    sample_rate: SampleRate,
    num_samples: Option<usize>,
}

impl Zero {
    /// Create a new source that never ends and produces total silence.
    #[inline]
    pub fn new(channels: ChannelCount, sample_rate: SampleRate) -> Zero {
        Zero {
            channels,
            sample_rate,
            num_samples: None,
        }
    }
    /// Create a new source that never ends and produces total silence.
    #[inline]
    pub fn new_samples(
        channels: ChannelCount,
        sample_rate: SampleRate,
        num_samples: usize,
    ) -> Zero {
        Zero {
            channels,
            sample_rate,
            num_samples: Some(num_samples),
        }
    }
}

impl Iterator for Zero {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(num_samples) = self.num_samples {
            if num_samples > 0 {
                self.num_samples = Some(num_samples - 1);
                Some(0.0)
            } else {
                None
            }
        } else {
            Some(0.0)
        }
    }
}

impl Source for Zero {
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.num_samples
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
    fn try_seek(&mut self, _: Duration) -> Result<(), SeekError> {
        Ok(())
    }
}
