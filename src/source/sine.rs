use crate::source::{Function, SignalGenerator};
use crate::Source;
use std::time::Duration;

use super::SeekError;

/// An infinite source that produces a sine.
///
/// Always has a sample rate of 48kHz and one channel.
///
/// This source is a thin interface on top of `SignalGenerator` provided for
/// your convenience.
#[derive(Clone, Debug)]
pub struct SineWave {
    test_sine: SignalGenerator,
}

impl SineWave {
    const SAMPLE_RATE: u32 = 48000;

    /// The frequency of the sine.
    #[inline]
    pub fn new(freq: f32) -> SineWave {
        let sr = cpal::SampleRate(Self::SAMPLE_RATE);
        SineWave {
            test_sine: SignalGenerator::new(sr, freq, Function::Sine),
        }
    }

    /// Create a new generator with given frequency and sample rate.
    pub fn new_with_sample_rate(freq: f32, sample_rate: u32) -> SineWave {
        assert!(
            sample_rate > (freq * 2.0) as u32,
            "frequency is too high for the sample rate"
        );
        let sr = cpal::SampleRate(sample_rate);
        SineWave {
            test_sine: SignalGenerator::new(sr, freq, Function::Sine),
        }
    }
}

impl Iterator for SineWave {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        self.test_sine.next()
    }
}

impl Source for SineWave {
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
        Self::SAMPLE_RATE
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    #[inline]
    fn try_seek(&mut self, duration: Duration) -> Result<(), SeekError> {
        self.test_sine.try_seek(duration)
    }
}
