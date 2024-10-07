//! Noise sources.
//!
//!

use crate::Source;

use super::SeekError;

use rand::{rngs::SmallRng, RngCore, SeedableRng};

/// Convenience function to create a new `WhiteNoise` noise source.
#[inline]
pub fn white(sample_rate: cpal::SampleRate) -> WhiteNoise {
    WhiteNoise::new(sample_rate)
}

/// Convenience function to create a new `PinkNoise` noise source.
#[inline]
pub fn pink(sample_rate: cpal::SampleRate) -> PinkNoise {
    PinkNoise::new(sample_rate)
}

/// Generates an infinite stream of random samples in [-1.0, 1.0]. This source generates random
/// samples as provided by the `rand::rngs::SmallRng` randomness source.
#[derive(Clone, Debug)]
pub struct WhiteNoise {
    sample_rate: cpal::SampleRate,
    rng: SmallRng,
}

impl WhiteNoise {
    /// Create a new white noise generator, seeding the RNG with `seed`.
    pub fn new_with_seed(sample_rate: cpal::SampleRate, seed: u64) -> Self {
        Self {
            sample_rate,
            rng: SmallRng::seed_from_u64(seed),
        }
    }

    /// Create a new white noise generator, seeding the RNG with system entropy.
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
        let rand = self.rng.next_u32() as f32 / u32::MAX as f32;
        let scaled = rand * 2.0 - 1.0;
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

/// Generates an infinite stream of pink noise samples in [-1.0, 1.0].
///
/// The output of the source is the result of taking the output of the `WhiteNoise` source and
/// filtering it according to a weighted-sum of seven FIR filters after [Paul Kellett's
/// method][pk_method] from *musicdsp.org*.
///
/// [pk_method]: https://www.musicdsp.org/en/latest/Filters/76-pink-noise-filter.html
pub struct PinkNoise {
    white_noise: WhiteNoise,
    b: [f32; 7],
}

impl PinkNoise {
    pub fn new(sample_rate: cpal::SampleRate) -> Self {
        Self {
            white_noise: WhiteNoise::new(sample_rate),
            b: [0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32],
        }
    }
}

impl Iterator for PinkNoise {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let white = self.white_noise.next().unwrap();
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
        self.white_noise.sample_rate()
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
