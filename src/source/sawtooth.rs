use crate::common::{ChannelCount, SampleRate};
use crate::source::{Function, SignalGenerator};
use crate::Source;
use std::time::Duration;

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
    const SAMPLE_RATE: SampleRate = 48000;

    /// The frequency of the sine.
    #[inline]
    pub fn new(freq: f32) -> SawtoothWave {
        SawtoothWave {
            test_saw: SignalGenerator::new(Self::SAMPLE_RATE, freq, Function::Sawtooth),
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
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        1
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        Self::SAMPLE_RATE
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    #[inline]
    fn try_seek(&mut self, duration: Duration) -> Result<(), SeekError> {
        self.test_saw.try_seek(duration)
    }
}
