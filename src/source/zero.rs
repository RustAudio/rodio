use std::marker::PhantomData;
use std::time::Duration;
use Sample;
use Source;

/// An infinite source that produces zero.
#[derive(Clone, Debug)]
pub struct Zero<S> {
    channels: u16,
    samples_rate: u32,
    marker: PhantomData<S>,
}

impl<S> Zero<S> {
    #[inline]
    pub fn new(channels: u16, samples_rate: u32) -> Zero<S> {
        Zero {
            channels: channels,
            samples_rate: samples_rate,
            marker: PhantomData,
        }
    }
}

impl<S> Iterator for Zero<S> where S: Sample {
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        Some(S::zero_value())
    }
}

impl<S> Source for Zero<S> where S: Sample {
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.channels
    }

    #[inline]
    fn samples_rate(&self) -> u32 {
        self.samples_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
