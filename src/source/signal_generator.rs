//! Generator sources for various periodic test waveforms.
//!
//! This module provides several periodic, deterministic waveforms for testing other sources and
//! for simple additive sound synthesis. Every source is monoaural and in the codomain [-1.0f32,
//! 1.0f32].
//!
//! # Example
//!
//! ```
//! use rodio::source::{SignalGenerator,Function};
//!
//! let tone = SignalGenerator::new(cpal::SampleRate(48000), 440.0, Function::Sine);
//! ```
use std::f32::consts::TAU;
use std::time::Duration;

use super::SeekError;
use crate::Source;

/// Waveform functions.
#[derive(Clone, Debug)]
pub enum Function {
    /// A sinusoidal waveform.
    Sine,
    /// A triangle waveform.
    Triangle,
    /// A square wave, rising edge at t=0.
    Square,
    /// A rising sawtooth wave.
    Sawtooth,
}

impl Function {
    /// Create a single sample for the given waveform
    #[inline]
    fn render(&self, i: u64, period: f32) -> f32 {
        let cycle_pos: f32 = i as f32 / period;

        match self {
            Self::Sine => (TAU * cycle_pos).sin(),
            Self::Triangle => 4.0f32 * (cycle_pos - (cycle_pos + 0.5f32).floor()).abs() - 1f32,
            Self::Square => {
                if cycle_pos % 1.0f32 < 0.5f32 {
                    1.0f32
                } else {
                    -1.0f32
                }
            }
            Self::Sawtooth => 2.0f32 * (cycle_pos - (cycle_pos + 0.5f32).floor()),
        }
    }
}

/// An infinite source that produces one of a selection of test waveforms.
#[derive(Clone, Debug)]
pub struct SignalGenerator {
    sample_rate: cpal::SampleRate,
    period: f32,
    function: Function,
    i: u64,
}

impl SignalGenerator {
    /// Create a new `TestWaveform` object that generates an endless waveform
    /// `f`.
    ///
    /// # Panics
    ///
    /// Will panic if `frequency` is equal to zero.
    #[inline]
    pub fn new(sample_rate: cpal::SampleRate, frequency: f32, f: Function) -> SignalGenerator {
        assert!(frequency != 0.0, "frequency must be greater than zero");
        let period = sample_rate.0 as f32 / frequency;
        SignalGenerator {
            sample_rate,
            period,
            function: f,
            i: 0,
        }
    }
}

impl Iterator for SignalGenerator {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        let val = Some(self.function.render(self.i, self.period));
        self.i += 1;
        val
    }
}

impl Source for SignalGenerator {
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
        self.sample_rate.0
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    #[inline]
    fn try_seek(&mut self, duration: Duration) -> Result<(), SeekError> {
        self.i = (self.sample_rate.0 as f32 * duration.as_secs_f32()) as u64;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::source::{Function, SignalGenerator};
    use approx::assert_abs_diff_eq;

    #[test]
    fn square() {
        let mut wf = SignalGenerator::new(cpal::SampleRate(2000), 500.0f32, Function::Square);
        assert_eq!(wf.next(), Some(1.0f32));
        assert_eq!(wf.next(), Some(1.0f32));
        assert_eq!(wf.next(), Some(-1.0f32));
        assert_eq!(wf.next(), Some(-1.0f32));
        assert_eq!(wf.next(), Some(1.0f32));
        assert_eq!(wf.next(), Some(1.0f32));
        assert_eq!(wf.next(), Some(-1.0f32));
        assert_eq!(wf.next(), Some(-1.0f32));
    }

    #[test]
    fn triangle() {
        let mut wf = SignalGenerator::new(cpal::SampleRate(8000), 1000.0f32, Function::Triangle);
        assert_eq!(wf.next(), Some(-1.0f32));
        assert_eq!(wf.next(), Some(-0.5f32));
        assert_eq!(wf.next(), Some(0.0f32));
        assert_eq!(wf.next(), Some(0.5f32));
        assert_eq!(wf.next(), Some(1.0f32));
        assert_eq!(wf.next(), Some(0.5f32));
        assert_eq!(wf.next(), Some(0.0f32));
        assert_eq!(wf.next(), Some(-0.5f32));
        assert_eq!(wf.next(), Some(-1.0f32));
        assert_eq!(wf.next(), Some(-0.5f32));
        assert_eq!(wf.next(), Some(0.0f32));
        assert_eq!(wf.next(), Some(0.5f32));
        assert_eq!(wf.next(), Some(1.0f32));
        assert_eq!(wf.next(), Some(0.5f32));
        assert_eq!(wf.next(), Some(0.0f32));
        assert_eq!(wf.next(), Some(-0.5f32));
    }

    #[test]
    fn saw() {
        let mut wf = SignalGenerator::new(cpal::SampleRate(200), 50.0f32, Function::Sawtooth);
        assert_eq!(wf.next(), Some(0.0f32));
        assert_eq!(wf.next(), Some(0.5f32));
        assert_eq!(wf.next(), Some(-1.0f32));
        assert_eq!(wf.next(), Some(-0.5f32));
        assert_eq!(wf.next(), Some(0.0f32));
        assert_eq!(wf.next(), Some(0.5f32));
        assert_eq!(wf.next(), Some(-1.0f32));
    }

    #[test]
    fn sine() {
        let mut wf = SignalGenerator::new(cpal::SampleRate(1000), 100f32, Function::Sine);

        assert_abs_diff_eq!(wf.next().unwrap(), 0.0f32);
        assert_abs_diff_eq!(wf.next().unwrap(), 0.58778525f32);
        assert_abs_diff_eq!(wf.next().unwrap(), 0.95105652f32);
        assert_abs_diff_eq!(wf.next().unwrap(), 0.95105652f32);
        assert_abs_diff_eq!(wf.next().unwrap(), 0.58778525f32);
        assert_abs_diff_eq!(wf.next().unwrap(), 0.0f32);
        assert_abs_diff_eq!(wf.next().unwrap(), -0.58778554f32);
    }
}
