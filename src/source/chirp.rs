//! Chirp/sweep source.

use std::{f32::consts::TAU, time::Duration};

use crate::Source;

/// Convenience function to create a new `Chirp` source.
#[inline]
pub fn chirp(
    sample_rate: cpal::SampleRate,
    start_frequency: f32,
    end_frequency: f32,
    duration: Duration,
) -> Chirp {
    Chirp::new(sample_rate, start_frequency, end_frequency, duration)
}

/// Generate a sine wave with an instantaneous frequency that changes/sweeps linearly over time.
/// At the end of the chirp, once the `end_frequency` is reached, the source is exhausted.
#[derive(Clone, Debug)]
pub struct Chirp {
    start_frequency: f32,
    end_frequency: f32,
    sample_rate: cpal::SampleRate,
    total_samples: u64,
    elapsed_samples: u64,
}

impl Chirp {
    fn new(
        sample_rate: cpal::SampleRate,
        start_frequency: f32,
        end_frequency: f32,
        duration: Duration,
    ) -> Self {
        Self {
            sample_rate,
            start_frequency,
            end_frequency,
            total_samples: (duration.as_secs_f64() * (sample_rate.0 as f64)) as u64,
            elapsed_samples: 0,
        }
    }
}

impl Iterator for Chirp {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let i = self.elapsed_samples;
        let ratio = self.elapsed_samples as f32 / self.total_samples as f32;
        self.elapsed_samples += 1;
        let freq = self.start_frequency * (1.0 - ratio) + self.end_frequency * ratio;
        let t = (i as f32 / self.sample_rate() as f32) * TAU * freq;
        Some(t.sin())
    }
}

impl Source for Chirp {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate.0
    }

    fn total_duration(&self) -> Option<Duration> {
        let secs: f64 = self.total_samples as f64 / self.sample_rate.0 as f64;
        Some(Duration::new(1, 0).mul_f64(secs))
    }
}
