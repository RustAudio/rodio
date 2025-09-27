//! Dithering for audio quantization and requantization.
//!
//! Dithering is a technique in digital audio processing that eliminates quantization
//! artifacts during various stages of audio processing. This module provides tools for
//! adding appropriate dither noise to maintain audio quality during quantization
//! operations.
//!
//! ## Example
//!
//! ```rust
//! use rodio::source::{dither, SineWave, DitherAlgorithm, Source};
//! use rodio::BitDepth;
//!
//! let source = SineWave::new(440.0);
//! let dithered = dither(source, BitDepth::new(16).unwrap(), DitherAlgorithm::TPDF);
//! ```
//!
//! ## Guidelines
//!
//! - **Apply dithering before volume changes** for optimal results
//! - **Dither once** - Apply only at the final output stage to avoid noise accumulation
//! - **Choose TPDF** for most professional audio applications (it's the default)
//! - **Use target output bit depth** - Not the source bit depth!
//!
//! When you later change volume (e.g., with `Sink::set_volume()`), both the signal
//! and dither noise scale together, maintaining proper dithering behavior.

use rand::{rngs::SmallRng, Rng};
use std::time::Duration;

use crate::{
    source::noise::{Blue, WhiteGaussian, WhiteTriangular, WhiteUniform},
    BitDepth, ChannelCount, Sample, SampleRate, Source,
};

/// Dither algorithm selection for runtime choice
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    /// GPDF (Gaussian PDF) - normal/bell curve distribution.
    ///
    /// Uses Gaussian white noise which more closely mimics natural processes and
    /// analog circuits. Higher noise floor than TPDF.
    GPDF,

    /// High-pass dithering - reduces low-frequency artifacts.
    ///
    /// Uses blue noise (high-pass filtered white noise) to push dither energy
    /// toward higher frequencies. Particularly effective for reducing audible
    /// low-frequency modulation artifacts.
    HighPass,

    /// RPDF (Rectangular PDF) - uniform distribution.
    ///
    /// Uses uniform white noise for basic decorrelation. Simpler than TPDF but
    /// allows some correlation between signal and quantization error at low levels.
    /// Slightly lower noise floor than TPDF.
    RPDF,

    /// TPDF (Triangular PDF) - triangular distribution.
    ///
    /// The gold standard for audio dithering. Provides mathematically optimal
    /// decorrelation by completely eliminating correlation between the original
    /// signal and quantization error.
    #[default]
    TPDF,
}

#[derive(Clone, Debug)]
#[allow(clippy::upper_case_acronyms)]
enum NoiseGenerator<R: Rng = SmallRng> {
    TPDF(WhiteTriangular<R>),
    RPDF(WhiteUniform<R>),
    GPDF(WhiteGaussian<R>),
    HighPass(Blue<R>),
}

impl NoiseGenerator {
    fn new(algorithm: Algorithm, sample_rate: SampleRate) -> Self {
        match algorithm {
            Algorithm::TPDF => Self::TPDF(WhiteTriangular::new(sample_rate)),
            Algorithm::RPDF => Self::RPDF(WhiteUniform::new(sample_rate)),
            Algorithm::GPDF => Self::GPDF(WhiteGaussian::new(sample_rate)),
            Algorithm::HighPass => Self::HighPass(Blue::new(sample_rate)),
        }
    }

    #[inline]
    fn next(&mut self) -> Option<Sample> {
        match self {
            Self::TPDF(gen) => gen.next(),
            Self::RPDF(gen) => gen.next(),
            Self::GPDF(gen) => gen.next(),
            Self::HighPass(gen) => gen.next(),
        }
    }

    fn algorithm(&self) -> Algorithm {
        match self {
            Self::TPDF(_) => Algorithm::TPDF,
            Self::RPDF(_) => Algorithm::RPDF,
            Self::GPDF(_) => Algorithm::GPDF,
            Self::HighPass(_) => Algorithm::HighPass,
        }
    }
}

/// A dithered audio source that applies quantization noise to reduce artifacts.
///
/// This struct wraps any audio source and applies dithering noise according to the
/// selected algorithm. Dithering is essential for digital audio playback and when
/// converting audio to different bit depths to prevent audible distortion.
///
/// # Example
///
/// ```rust
/// use rodio::source::{SineWave, dither, DitherAlgorithm};
/// use rodio::BitDepth;
///
/// let source = SineWave::new(440.0);
/// let dithered = dither(source, BitDepth::new(16).unwrap(), DitherAlgorithm::TPDF);
/// ```
#[derive(Clone, Debug)]
pub struct Dither<I> {
    input: I,
    noise: NoiseGenerator,
    lsb_amplitude: f32,
}

impl<I> Dither<I>
where
    I: Source,
{
    /// Creates a new dithered source with the specified algorithm
    pub fn new(input: I, target_bits: BitDepth, algorithm: Algorithm) -> Self {
        // LSB amplitude for signed audio: 1.0 / (2^(bits-1))
        // For high bit depths (> mantissa precision), we're limited by the sample type's
        // mantissa bits. Instead of dithering to a level that would be truncated,
        // we dither at the actual LSB level representable by the sample format.
        let lsb_amplitude = if target_bits.get() >= Sample::MANTISSA_DIGITS {
            Sample::MIN_POSITIVE
        } else {
            1.0 / (1_i64 << (target_bits.get() - 1)) as f32
        };

        let sample_rate = input.sample_rate();
        Self {
            input,
            noise: NoiseGenerator::new(algorithm, sample_rate),
            lsb_amplitude,
        }
    }

    /// Change the dithering algorithm at runtime
    /// This recreates the noise generator with the new algorithm
    pub fn set_algorithm(&mut self, algorithm: Algorithm) {
        if self.noise.algorithm() != algorithm {
            let sample_rate = self.input.sample_rate();
            self.noise = NoiseGenerator::new(algorithm, sample_rate);
        }
    }

    /// Get the current dithering algorithm
    #[inline]
    pub fn algorithm(&self) -> Algorithm {
        self.noise.algorithm()
    }
}

impl<I> Iterator for Dither<I>
where
    I: Source,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let input_sample = self.input.next()?;
        let noise_sample = self.noise.next().unwrap_or(0.0);

        // Apply subtractive dithering at the target quantization level
        Some(input_sample - noise_sample * self.lsb_amplitude)
    }
}

impl<I> Source for Dither<I>
where
    I: Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.input.channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), crate::source::SeekError> {
        self.input.try_seek(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{SineWave, Source};
    use crate::{nz, BitDepth, SampleRate};

    const TEST_SAMPLE_RATE: SampleRate = nz!(44100);
    const TEST_BIT_DEPTH: BitDepth = nz!(16);

    #[test]
    fn test_dither_adds_noise() {
        let source = SineWave::new(440.0).take_duration(std::time::Duration::from_millis(10));
        let mut dithered = Dither::new(source.clone(), TEST_BIT_DEPTH, Algorithm::TPDF);
        let mut undithered = source;

        // Collect samples from both sources
        let dithered_samples: Vec<f32> = (0..10).filter_map(|_| dithered.next()).collect();
        let undithered_samples: Vec<f32> = (0..10).filter_map(|_| undithered.next()).collect();

        let lsb = 1.0 / (1_i64 << (TEST_BIT_DEPTH.get() - 1)) as f32;

        // Verify dithered samples differ from undithered and are reasonable
        for (i, (&dithered_sample, &undithered_sample)) in dithered_samples
            .iter()
            .zip(undithered_samples.iter())
            .enumerate()
        {
            // Should be finite
            assert!(
                dithered_sample.is_finite(),
                "Dithered sample {} should be finite",
                i
            );

            // The difference should be small (just dither noise)
            let diff = (dithered_sample - undithered_sample).abs();
            let max_expected_diff = lsb * 2.0; // Max triangular dither amplitude
            assert!(
                diff <= max_expected_diff,
                "Dither noise too large: sample {}, diff {}, max expected {}",
                i,
                diff,
                max_expected_diff
            );
        }
    }
}
