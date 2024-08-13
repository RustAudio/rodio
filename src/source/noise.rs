use crate::Source;

use super::SeekError;

use rand::rngs::SmallRng;
use rand::{RngCore, SeedableRng};

/// Create a new white noise source.
#[inline]
pub fn white(sample_rate: cpal::SampleRate) -> WhiteNoise {
    WhiteNoise::new(sample_rate)
}

/// Create a new pink noise source.
#[inline]
pub fn pink(sample_rate: cpal::SampleRate) -> PinkNoise {
    PinkNoise::new(sample_rate)
}

/// Generates an infinite stream of random samples in [=1.0, 1.0]
#[derive(Clone, Debug)]
pub struct WhiteNoise {
    sample_rate: cpal::SampleRate,
    rng: SmallRng,
}

impl WhiteNoise {
    /// Create a new white noise generator.
    pub fn new(sample_rate: cpal::SampleRate) -> Self {
        Self {
            sample_rate,
            rng: SmallRng::from_entropy(),
        }
    }
}

impl Iterator for WhiteNoise {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let randf = self.rng.next_u32() as f32 / u32::MAX as f32;
        let scaled = randf * 2.0 - 1.0;
        Some(scaled)
    }
}

impl Source for WhiteNoise {
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
    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }

    #[inline]
    fn try_seek(&mut self, _: std::time::Duration) -> Result<(), SeekError> {
        // Does nothing, should do nothing
        Ok(())
    }
}

// https://www.musicdsp.org/en/latest/Filters/76-pink-noise-filter.html
//
/// Generate an infinite stream of pink noise samples in [-1.0, 1.0].
pub struct PinkNoise {
    noise: WhiteNoise,
    b: [f32; 7],
}

impl PinkNoise {
    pub fn new(sample_rate: cpal::SampleRate) -> Self {
        Self {
            noise: WhiteNoise::new(sample_rate),
            b: [0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32],
        }
    }
}

impl Iterator for PinkNoise {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let white = self.noise.next().unwrap();
        self.b[0] = 0.99886 * self.b[0] + white * 0.0555179;
        self.b[1] = 0.99332 * self.b[1] + white * 0.0750759;
        self.b[2] = 0.969 * self.b[2] + white * 0.153852;
        self.b[3] = 0.8665 * self.b[3] + white * 0.3104856;
        self.b[4] = 0.550 * self.b[4] + white * 0.5329522;
        self.b[5] = -0.7616 * self.b[5] - white * 0.016898;

        let pink = self.b[0]
            + self.b[1]
            + self.b[2]
            + self.b[3]
            + self.b[4]
            + self.b[5]
            + self.b[6]
            + white * 0.5362;

        self.b[6] = white * 0.115926;

        Some(pink)
    }
}

impl Source for PinkNoise {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        self.noise.sample_rate()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }

    #[inline]
    fn try_seek(&mut self, _: std::time::Duration) -> Result<(), SeekError> {
        // Does nothing, should do nothing
        Ok(())
    }
}
