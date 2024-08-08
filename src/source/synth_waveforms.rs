use std::f32::consts::TAU;
use std::time::Duration;

use super::SeekError;
use crate::{buffer::SamplesBuffer, source::Repeat, Source};


/// Express a frequency as a rational number.
///     .0 Cycles per time quanta
///     .1 Time quanta per second
///
/// Examples:
///     1000,1      1000 Hz
///     12345,100   123.45 Hz
pub struct RationalFrequency(u32, u32);

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
    /// Create a single sample for the given waveform
    #[inline]
    pub fn render(&self, sample: u32, period: u32) -> f32 {
        let i_div_p: f32 = sample as f32 / period as f32;
            
        match self {
            Self::Sine => (TAU * i_div_p).sin(),
            Self::Triangle => 04.0f32 * (i_div_p - (i_div_p + 0.5f32).floor()).abs() - 1f32,
            Self::Square => {
                    if i_div_p < 0.5f32 {
                        1.0f32
                    } else {
                        -1.0f32
                    }
                },
            Self::Sawtooth => 2.0f32 * (i_div_p - (i_div_p + 0.5f32).floor()),
        }
    }

    /// Create a `SamplesBuffer` containing one period of `self` with the given
    /// sample rate and frequency.
    pub fn create_buffer(
        &self,
        sample_rate: cpal::SampleRate,
        frequency: u32,
    ) -> SamplesBuffer<f32> {
        let period: usize = (sample_rate.0 / frequency) as usize;
        let mut samples_vec = vec![0.0f32; period];

        for i in 0..period {
            samples_vec[i] = self.render(i as u32, period as u32);
        }

        SamplesBuffer::new(1, sample_rate.0, samples_vec)
    }
}

/// An infinite source that produces one of a selection of synthesizer
/// waveforms from a buffered source.
#[derive(Clone)]
pub struct BufferedSynthWaveform {
    input: Repeat<SamplesBuffer<f32>>,
}

impl BufferedSynthWaveform {
    /// Create a new `SynthWaveform` object that generates an endless waveform
    /// `f`.
    #[inline]
    pub fn new(
        sample_rate: cpal::SampleRate,
        frequency: u32,
        f: SynthWaveformFunction,
    ) -> BufferedSynthWaveform {
        let buffer = f.create_buffer(sample_rate, frequency);
        BufferedSynthWaveform {
            input: buffer.repeat_infinite(),
        }
    }
}

impl Iterator for BufferedSynthWaveform {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        self.input.next()
    }
}

impl Source for BufferedSynthWaveform {
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
        let mut wf =
            BufferedSynthWaveform::new(cpal::SampleRate(1000), 250, SynthWaveformFunction::Square);
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
        let mut wf = BufferedSynthWaveform::new(
            cpal::SampleRate(8000),
            1000,
            SynthWaveformFunction::Triangle,
        );
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
        let mut wf =
            BufferedSynthWaveform::new(cpal::SampleRate(200), 50, SynthWaveformFunction::Sawtooth);
        assert_eq!(wf.next(), Some(0.0f32));
        assert_eq!(wf.next(), Some(0.5f32));
        assert_eq!(wf.next(), Some(-1.0f32));
        assert_eq!(wf.next(), Some(-0.5f32));
        assert_eq!(wf.next(), Some(0.0f32));
        assert_eq!(wf.next(), Some(0.5f32));
        assert_eq!(wf.next(), Some(-1.0f32));
    }
}
