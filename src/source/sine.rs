use crate::constants::DEFAULT_SAMPLE_RATE;
use crate::source::{Function, SignalGenerator};
use crate::Source;
use std::time::Duration;

use super::SeekError;

/// An infinite source that produces a sine.
///
/// Always has default sample rate.
///
/// This source is a thin interface on top of `SignalGenerator` provided for
/// your convenience.
#[derive(Clone, Debug)]
pub struct SineWave {
    test_sine: SignalGenerator,
}

impl SineWave {
    /// The frequency of the sine.
    #[inline]
    pub fn new(freq: f32) -> SineWave {
        let sr = cpal::SampleRate(DEFAULT_SAMPLE_RATE);
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
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> u16 {
        1
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.test_sine.sample_rate()
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
