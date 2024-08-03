use std::time::Duration;

use super::SeekError;
use crate::{Sample, Source};

/// Internal function that builds a `LinearRamp` object.
pub fn linear_gain_ramp<I>(
    input: I,
    duration: Duration,
    start_gain: f32,
    end_gain: f32,
    clamp_end: bool,
) -> LinearGainRamp<I>
where
    I: Source,
    I::Item: Sample,
{
    let duration_nanos = duration.as_nanos() as f32;
    assert!(duration_nanos > 0.0f32);

    LinearGainRamp {
        input,
        elapsed_ns: 0.0f32,
        total_ns: duration_nanos,
        start_gain,
        end_gain,
        clamp_end,
        sample_idx: 0u64,
    }
}

/// Filter that adds a linear gain ramp to the source over a given time range.
#[derive(Clone, Debug)]
pub struct LinearGainRamp<I> {
    input: I,
    elapsed_ns: f32,
    total_ns: f32,
    start_gain: f32,
    end_gain: f32,
    clamp_end: bool,
    sample_idx: u64
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
        let remaining_ns = self.total_ns - self.elapsed_ns;

        if remaining_ns < 0.0 {
            if self.clamp_end {
                factor = self.end_gain;
            } else {
                factor = 1.0f32;
            }
        } else {
            self.sample_idx += 1;
            
            let p = self.elapsed_ns / self.total_ns;
            factor = self.start_gain * (1.0f32 - p)  + self.end_gain * p;
        }

        if self.sample_idx % (self.channels() as u64) == 0 {
            self.elapsed_ns +=
                1000000000.0 / (self.input.sample_rate() as f32);
        }


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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::SamplesBuffer;

    fn dummysource(length: u8) -> SamplesBuffer<f32> {
        // shamelessly copied from crossfade.rs
        let data: Vec<f32> = (1..=length).map(f32::from).collect();
        SamplesBuffer::new(1, 1, data)
    }

    #[test]
    fn test_linearramp() {
        let source1 = dummysource(10);
        let mut faded = linear_gain_ramp(source1, 
                                         Duration::from_secs(4), 
                                         0.0, 1.0, true);

        assert_eq!(faded.next(), Some(0.0));
        assert_eq!(faded.next(), Some(0.5));
        assert_eq!(faded.next(), Some(1.5));
        assert_eq!(faded.next(), Some(3.0));
        assert_eq!(faded.next(), Some(5.0));
        assert_eq!(faded.next(), Some(6.0));
        assert_eq!(faded.next(), Some(7.0));
        assert_eq!(faded.next(), Some(8.0));
        assert_eq!(faded.next(), Some(9.0));
        assert_eq!(faded.next(), Some(10.0));
        assert_eq!(faded.next(), None);
    }

    #[test]
    fn test_linearramp_clamped() {
        let source1 = dummysource(10);
        let mut faded = linear_gain_ramp(source1, 
                                         Duration::from_secs(4), 
                                         0.0, 0.5, true);

        assert_eq!(faded.next(), Some(0.0));
        assert_eq!(faded.next(), Some(0.25));
        assert_eq!(faded.next(), Some(0.75));
        assert_eq!(faded.next(), Some(1.5));
        assert_eq!(faded.next(), Some(2.5));
        assert_eq!(faded.next(), Some(3.0));
        assert_eq!(faded.next(), Some(3.5));
        assert_eq!(faded.next(), Some(4.0));
        assert_eq!(faded.next(), Some(4.5));
        assert_eq!(faded.next(), Some(5.0));
        assert_eq!(faded.next(), None);
    }
}
