use std::marker::PhantomData;
use std::time::Duration;

use crate::{Sample, Source};

/// An infinite source that produces zero.
#[derive(Clone, Debug)]
pub struct Zero<S> {
    channels: u16,
    sample_rate: u32,
    num_samples: Option<usize>,
    marker: PhantomData<S>,
}

impl<S> Zero<S> {
    #[inline]
    pub fn new(channels: u16, sample_rate: u32) -> Zero<S> {
        Zero {
            channels,
            sample_rate,
            num_samples: None,
            marker: PhantomData,
        }
    }
    #[inline]
    pub fn new_samples(channels: u16, sample_rate: u32, num_samples: usize) -> Zero<S> {
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
    fn current_frame_len(&self) -> Option<usize> {
        self.num_samples
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.channels
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
