use std::f32::consts::TAU;
use std::time::Duration;

use super::SeekError;
use crate::{buffer::SamplesBuffer, source::Repeat, Source};

/// Syntheizer waveform functions. All of the synth waveforms are in the
/// codomain [-1.0, 1.0].
#[derive(Clone, Debug)]
pub enum SynthWaveformFunction {
    Sine,
    Triangle,
    Square,
    Sawtooth,
}

impl SynthWaveformFunction {
    /// Create a `SamplesBuffer` containing one period of `self` with the given
    /// sample rate and frequency.
    pub fn create_buffer(&self, sample_rate: u32, frequency: u32) -> SamplesBuffer<f32> {
        let p: usize = (sample_rate / frequency) as usize;
        let mut samples_vec = vec![0.0f32; p];

        fn _pwm_impl(duty: f32, t: f32) -> f32 {
            if t < duty.abs() {
                1f32
            } else {
                -1f32
            }
        }

        for i in 0..p {
            let i_div_p: f32 = i as f32 / p as f32;
            samples_vec[i] = match self {
                Self::Sine => (TAU * i_div_p).sin(),
                Self::Triangle => 4.0f32 * (i_div_p - (i_div_p + 0.5f32).floor()).abs() - 1f32,
                Self::Square => {
                    if i_div_p < 0.5f32 {
                        1.0f32
                    } else {
                        -1.0f32
                    }
                }
                Self::Sawtooth => 2.0f32 * (i_div_p - (i_div_p + 0.5f32).floor()),
            }
        }

        SamplesBuffer::new(1, sample_rate, samples_vec)
    }
}

/// An infinite source that produces one of a selection of synthesizer
/// waveforms from a buffered source.
#[derive(Clone)]
pub struct SynthWaveform {
    input: Repeat<SamplesBuffer<f32>>,
}

impl SynthWaveform {
    /// Create a new `SynthWaveform` object that generates an endless waveform
    /// `f`.
    #[inline]
    pub fn new(sample_rate: u32, frequency: u32, f: SynthWaveformFunction) -> SynthWaveform {
        let buffer = f.create_buffer(sample_rate, frequency);
        SynthWaveform {
            input: buffer.repeat_infinite(),
        }
    }
}

impl Iterator for SynthWaveform {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        self.input.next()
    }
}

impl Source for SynthWaveform {
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        self.input.current_frame_len()
    }

    #[inline]
    fn channels(&self) -> u16 {
        1
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    #[inline]
    fn try_seek(&mut self, duration: Duration) -> Result<(), SeekError> {
        self.input.try_seek(duration)
    }
}

#[cfg(test)]
mod tests {
    use crate::source::synth_waveforms::*;

    #[test]
    fn square() {
        let mut wf = SynthWaveform::new(1000, 250, SynthWaveformFunction::Square);
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
        let mut wf = SynthWaveform::new(8000, 1000, SynthWaveformFunction::Triangle);
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
        let mut wf = SynthWaveform::new(200, 50, SynthWaveformFunction::Sawtooth);
        assert_eq!(wf.next(), Some(0.0f32));
        assert_eq!(wf.next(), Some(0.5f32));
        assert_eq!(wf.next(), Some(-1.0f32));
        assert_eq!(wf.next(), Some(-0.5f32));
        assert_eq!(wf.next(), Some(0.0f32));
        assert_eq!(wf.next(), Some(0.5f32));
        assert_eq!(wf.next(), Some(-1.0f32));
    }
}
