use std::time::Duration;

use crate::source::{Function, SignalGenerator};
use crate::Source;

use super::SeekError;

/// An infinite source that produces a triangle wave.
///
/// Always has a sample rate of 48kHz and one channel.
///
/// This source is a thin interface on top of `SignalGenerator` provided for
/// your convenience.
#[derive(Clone, Debug)]
pub struct TriangleWave {
    test_tri: SignalGenerator,
}

impl TriangleWave {
    const SAMPLE_RATE: u32 = 48000;

    /// The frequency of the sine.
    #[inline]
    pub fn new(freq: f32) -> TriangleWave {
        let sr = cpal::SampleRate(Self::SAMPLE_RATE);
        TriangleWave {
            test_tri: SignalGenerator::new(sr, freq, Function::Triangle),
        }
    }
}

impl Iterator for TriangleWave {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        self.test_tri.next()
    }
}

impl Source for TriangleWave {
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
        self.test_tri.try_seek(duration)
    }
}
