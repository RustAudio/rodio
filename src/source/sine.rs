use std::time::Duration;

use crate::source::{TestWaveform, TestWaveformFunction};
use crate::Source;

use super::SeekError;

const SAMPLE_RATE: u32 = 48000;

/// An infinite source that produces a sine.
///
/// Always has a rate of 48kHz and one channel.
#[derive(Clone, Debug)]
pub struct SineWave {
    test_sine: TestWaveform,
}

impl SineWave {
    /// The frequency of the sine.
    #[inline]
    pub fn new(freq: f32) -> SineWave {
        let sr = cpal::SampleRate(SAMPLE_RATE);
        SineWave {
            test_sine: TestWaveform::new(sr, freq, TestWaveformFunction::Sine),
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
        SAMPLE_RATE
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.test_sine.try_seek(pos)
    }
}
