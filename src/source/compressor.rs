use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::Source;

/// Internal function that builds a `Compressor` object.
pub(crate) fn compressor<I>(
    input: I,
    threshold: f32,
    ratio: f32,
    attack: f32,
    release: f32,
) -> Compressor<I>
where
    I: Source,
{
    Compressor {
        input,
        threshold,
        ratio,
        attack,
        release,
        gain: 1.0,
        envelope: 0.0,
    }
}

/// Filter that applies a compressor effect to the source.
#[derive(Clone, Debug)]
pub struct Compressor<I> {
    input: I,
    threshold: f32,
    ratio: f32,
    attack: f32,
    release: f32,
    gain: f32,
    envelope: f32,
}

impl<I> Compressor<I> {
    /// Set the compression threshold.
    #[inline]
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold;
    }

    /// Set the compression ratio.
    #[inline]
    pub fn set_ratio(&mut self, ratio: f32) {
        self.ratio = ratio;
    }

    /// Set the attack time (seconds).
    #[inline]
    pub fn set_attack(&mut self, attack: f32) {
        self.attack = attack;
    }

    /// Set the release time (seconds).
    #[inline]
    pub fn set_release(&mut self, release: f32) {
        self.release = release;
    }

    /// Returns a reference to the inner source.
    #[inline]
    pub fn inner(&self) -> &I {
        &self.input
    }

    /// Returns a mutable reference to the inner source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.input
    }

    /// Returns the inner source.
    #[inline]
    pub fn into_inner(self) -> I {
        self.input
    }
}

impl<I> Iterator for Compressor<I>
where
    I: Source,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.input.next()?;
        let abs_sample = sample.abs();

        // Envelope follower (simple peak detector)
        let sr = self.input.sample_rate() as f32;
        let attack_coeff = f32::exp(-1.0 / (self.attack * sr).max(1.0));
        let release_coeff = f32::exp(-1.0 / (self.release * sr).max(1.0));
        if abs_sample > self.envelope {
            self.envelope = attack_coeff * (self.envelope - abs_sample) + abs_sample;
        } else {
            self.envelope = release_coeff * (self.envelope - abs_sample) + abs_sample;
        }

        // Compute gain reduction
        let mut gain = 1.0;
        if self.envelope > self.threshold {
            let over = self.envelope / self.threshold;
            gain = (1.0 + (over - 1.0) / self.ratio).recip();
        }
        self.gain = gain;

        Some(sample * self.gain)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for Compressor<I> where I: Source + ExactSizeIterator {}

impl<I> Source for Compressor<I>
where
    I: Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.input.channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.input.try_seek(pos)
    }
}
