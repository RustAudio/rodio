use std::time::Duration;

use crate::{Sample, Source};
use super::SeekError;

/// Internal function that builds a `LinearRamp` object.
pub fn linear_gain_ramp<I>(
    input: I,
    duration: Duration,
    start_gain: f32,
    end_gain: f32,
    clamp_end: bool
) -> LinearGainRamp<I>
where
    I: Source,
    I::Item: Sample,
{
    let duration_nanos = duration.as_secs() * 1000000000 + duration.subsec_nanos() as u64;
    assert!(duration_nanos > 0);

    LinearGainRamp {
        input,
        remaining_ns: duration_nanos as f32,
        total_ns: duration_nanos as f32,
        start_gain,
        end_gain,
        clamp_end
    }
}

/// Filter that adds a linear gain ramp to the source over a given time range.
#[derive(Clone, Debug)]
pub struct LinearGainRamp<I> {
    input: I,
    remaining_ns: f32,
    total_ns: f32,
    start_gain: f32,
    end_gain: f32,
    clamp_end: bool,
}

impl<I> LinearGainRamp<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Returns a reference to the innner source.
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

impl<I> Iterator for LinearGainRamp<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        let factor: f32; 

        if self.remaining_ns <= 0.0 {
            if self.clamp_end {
                factor = self.end_gain;
            } else {
                factor = 1.0f32;
            }
        } else {
            factor = f32::lerp(
                self.start_gain,
                self.end_gain,
                self.remaining_ns as u32,
                self.total_ns as u32,
            );
        }

        self.remaining_ns -=
            1000000000.0 / (self.input.sample_rate() as f32 * self.channels() as f32);

        self.input.next().map(|value| value.amplify(factor))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for LinearGainRamp<I>
where
    I: Source + ExactSizeIterator,
    I::Item: Sample,
{
}

impl<I> Source for LinearGainRamp<I>
where
    I: Source,
    I::Item: Sample,
{ 
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        self.input.current_frame_len()
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.input.channels()
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
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
