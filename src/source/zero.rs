use std::marker::PhantomData;
use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::{Sample, Source};

/// An source that produces samples with value zero (silence). Depending on if
/// it where created with [`Zero::new`] or [`Zero::new_samples`] it can be never
/// ending or finite.
#[derive(Clone, Debug)]
pub struct Zero<S> {
    channels: ChannelCount,
    sample_rate: SampleRate,
    num_samples: Option<usize>,
    marker: PhantomData<S>,
}

impl<S> Zero<S> {
    /// Create a new source that never ends and produces total silence.
    #[inline]
    pub fn new(channels: ChannelCount, sample_rate: SampleRate) -> Zero<S> {
        Zero {
            channels,
            sample_rate,
            num_samples: None,
            marker: PhantomData,
        }
    }
    /// Create a new source that never ends and produces total silence.
    #[inline]
    pub fn new_samples(
        channels: ChannelCount,
        sample_rate: SampleRate,
        num_samples: usize,
    ) -> Zero<S> {
        Zero {
            channels,
            sample_rate,
            num_samples: Some(num_samples),
            marker: PhantomData,
        }
    }
}

impl<S> Iterator for Zero<S>
where
    S: Sample,
{
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        if let Some(num_samples) = self.num_samples {
            if num_samples > 0 {
                self.num_samples = Some(num_samples - 1);
                Some(S::zero_value())
            } else {
                None
            }
        } else {
            Some(S::zero_value())
        }
    }
}

impl<S> Source for Zero<S>
where
    S: Sample,
{
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
