use std::time::Duration;

use crate::source::{Function, SignalGenerator};
use crate::Source;

use super::SeekError;

/// An infinite source that produces a square wave.
///
/// Always has a sample rate of 48kHz and one channel.
///
/// This source is a thin interface on top of `SignalGenerator` provided for
/// your convenience.
#[derive(Clone, Debug)]
pub struct SquareWave {
    test_square: SignalGenerator,
}

impl SquareWave {
    const SAMPLE_RATE: u32 = 48000;

    /// The frequency of the sine.
    #[inline]
    pub fn new(freq: f32) -> SquareWave {
        let sr = cpal::SampleRate(Self::SAMPLE_RATE);
        SquareWave {
            test_square: SignalGenerator::new(sr, freq, Function::Square),
        }
    }
}

impl Iterator for SquareWave {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        self.test_square.next()
    }
}

impl Source for SquareWave {
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

    /// `try_seek()` does nothing on the squarewave generator. If you need to
    /// generate a test signal with a precise phase or sample offset, consider
    /// using `skip::skip_samples()`.
    #[inline]
    fn try_seek(&mut self, _: Duration) -> Result<(), SeekError> {
        Ok(())
    }
}
