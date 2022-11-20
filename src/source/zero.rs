use std::marker::PhantomData;
use std::time::Duration;

use crate::{Sample, Source};

/// A source that produces zero.
#[derive(Clone, Debug)]
pub struct Zero<S> {
    channels: u16,
    sample_rate: u32,
    /// The number of samples to produce and the total duration.
    /// If `None`, will be infinite.
    len: Option<(usize, Duration)>,
    marker: PhantomData<S>,
}

impl<S> Zero<S> {
    #[inline]
    pub fn new(channels: u16, sample_rate: u32) -> Zero<S> {
        Zero {
            channels,
            sample_rate,
            len: None,
            marker: PhantomData,
        }
    }

    pub fn new_finite(channels: u16, sample_rate: u32, duration: Duration) -> Zero<S> {
        let len = Some((duration.as_secs_f32() * sample_rate as f32 * channels as f32) as usize);
        Zero {
            channels,
            sample_rate,
            len: len.map(|samples| (samples, duration)),
            marker: PhantomData,
        }
    }

    #[inline]
    fn samples_left(&self) -> Option<usize> {
        self.len.map(|len| len.0)
    }
}

impl<S> Iterator for Zero<S>
where
    S: Sample,
{
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        match self.len {
            None => Some(S::zero_value()),
            Some((0, _)) => None,
            Some((samples, duration)) => {
                self.len = Some((samples - 1, duration));
                Some(S::zero_value())
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.samples_left().unwrap_or(0), self.samples_left())
    }
}

impl<S> Source for Zero<S>
where
    S: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        self.samples_left()
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
        self.len.map(|len| len.1)
    }
}
