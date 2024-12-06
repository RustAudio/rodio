use std::time::Duration;

use crate::source::{Function, SignalGenerator};
use crate::Source;

use super::SeekError;

/// An infinite source that produces a sawtooth wave.
///
/// Always has a sample rate of 48kHz and one channel.
///
/// This source is a thin interface on top of `SignalGenerator` provided for
/// your convenience.
#[derive(Clone, Debug)]
pub struct SawtoothWave {
    test_saw: SignalGenerator,
}

impl SawtoothWave {
    const SAMPLE_RATE: u32 = 48000;

    /// The frequency of the sine.
    #[inline]
    pub fn new(freq: f32) -> SawtoothWave {
        let sr = cpal::SampleRate(Self::SAMPLE_RATE);
        SawtoothWave {
            test_saw: SignalGenerator::new(sr, freq, Function::Sawtooth),
        }
    }
}

impl Iterator for SawtoothWave {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        self.test_saw.next()
    }
}

impl Source for SawtoothWave {
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

    /// `try_seek()` does nothing on the sawtooth generator. If you need to
    /// generate a test signal with a precise phase or sample offset, consider
    /// using `skip::skip_samples()`.
    #[inline]
    fn try_seek(&mut self, _: Duration) -> Result<(), SeekError> {
        Ok(())
    }
}
