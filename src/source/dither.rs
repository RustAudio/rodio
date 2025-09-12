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
//! use rodio::source::{dither, SineWave};
//! use rodio::source::DitherAlgorithm;
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
//! - **Use HighPass** for material with audible low-frequency dither artifacts
//! - **Use target output bit depth** - Not the source bit depth!
//!
//! When you later change volume (e.g., with `Sink::set_volume()`), both the signal
//! and dither noise scale together, maintaining proper dithering behavior.

use std::time::Duration;

use rand::{rngs::SmallRng, Rng, SeedableRng};

use crate::{
    source::noise::{Blue, WhiteGaussian, WhiteTriangular, WhiteUniform},
    BitDepth, ChannelCount, Sample, SampleRate, Source,
};

/// Dither algorithm selection - chooses the probability density function (PDF).
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
    /// low-frequency modulation artifacts. Best for material with significant
    /// low-frequency content where traditional white dither might be audible.
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

/// Internal dithering implementation with a specific noise generator type.
#[derive(Clone, Debug)]
pub struct DitherImpl<I, N> {
    input: I,
    noise: N,
    target_bits: BitDepth,
    lsb_amplitude: f32,
}

impl<I, N> DitherImpl<I, N>
where
    I: Source,
    N: Iterator<Item = Sample>,
{
    /// Creates a new dither source with a custom noise generator.
    ///
    /// This low-level internal constructor allows providing a custom noise generator.
    /// The noise generator should produce samples with appropriate amplitude
    /// for the chosen dither type.
    #[inline]
    pub(crate) fn new_with_noise(input: I, noise: N, target_bits: BitDepth) -> Self {
        // LSB amplitude for signed audio: 1.0 / (2^(bits-1))
        // This represents the amplitude of one quantization level
        // Use i64 bit shifting to avoid overflow (supports up to 63 bits)
        let lsb_amplitude = 1.0 / (1_i64 << (target_bits.get() - 1)) as f32;

        Self {
            input,
            noise,
            target_bits,
            lsb_amplitude,
        }
    }
}

impl<I, N> Iterator for DitherImpl<I, N>
where
    I: Source,
    N: Iterator<Item = Sample>,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let input_sample = self.input.next()?;
        let noise_sample = self.noise.next().unwrap_or(0.0);

        // Add dither noise at the target quantization level
        let dithered = input_sample + noise_sample * self.lsb_amplitude;

        Some(dithered)
    }
}

impl<I, N> Source for DitherImpl<I, N>
where
    I: Source,
    N: Iterator<Item = Sample>,
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
    fn bits_per_sample(&self) -> Option<BitDepth> {
        Some(self.target_bits)
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), crate::source::SeekError> {
        self.input.try_seek(pos)
    }
}

/// Dithering interface delegating to the supported dithering algorithms.
#[derive(Clone)]
pub enum Dither<I, R = SmallRng>
where
    R: Rng + SeedableRng + Clone,
{
    /// GPDF dithering with Gaussian white noise
    GPDF(DitherImpl<I, WhiteGaussian<R>>),

    /// High-pass dithering with blue noise
    HighPass(DitherImpl<I, Blue<R>>),

    /// RPDF dithering with uniform white noise
    RPDF(DitherImpl<I, WhiteUniform<R>>),

    /// TPDF dithering with triangular white noise
    TPDF(DitherImpl<I, WhiteTriangular<R>>),
}

impl<I, R> Dither<I, R>
where
    I: Source,
    R: Rng + SeedableRng + Clone,
{
}

impl<I> Dither<I, SmallRng>
where
    I: Source,
{
    /// Creates a new dithered source using the specified algorithm.
    ///
    /// This is the main constructor for dithering. Choose the algorithm based on your needs:
    /// - `GPDF`: Natural/analog-like characteristics
    /// - `HighPass`: Reduces low-frequency dither artifacts
    /// - `RPDF`: Lower noise floor but some correlation
    /// - `TPDF` (default): Optimal decorrelation
    #[inline]
    pub fn new(input: I, target_bits: BitDepth, algorithm: Algorithm) -> Self {
        let sample_rate = input.sample_rate();
        match algorithm {
            Algorithm::GPDF => {
                let noise = WhiteGaussian::new(sample_rate);
                Self::GPDF(DitherImpl::new_with_noise(input, noise, target_bits))
            }
            Algorithm::HighPass => {
                let noise = Blue::new(sample_rate);
                Self::HighPass(DitherImpl::new_with_noise(input, noise, target_bits))
            }
            Algorithm::RPDF => {
                let noise = WhiteUniform::new(sample_rate);
                Self::RPDF(DitherImpl::new_with_noise(input, noise, target_bits))
            }
            Algorithm::TPDF => {
                let noise = WhiteTriangular::new(sample_rate);
                Self::TPDF(DitherImpl::new_with_noise(input, noise, target_bits))
            }
        }
    }
}

impl<I, R> Iterator for Dither<I, R>
where
    I: Source,
    R: Rng + SeedableRng + Clone,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Dither::GPDF(d) => d.next(),
            Dither::HighPass(d) => d.next(),
            Dither::RPDF(d) => d.next(),
            Dither::TPDF(d) => d.next(),
        }
    }
}

impl<I, R> Source for Dither<I, R>
where
    I: Source,
    R: Rng + SeedableRng + Clone,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        match self {
            Dither::GPDF(d) => d.current_span_len(),
            Dither::HighPass(d) => d.current_span_len(),
            Dither::RPDF(d) => d.current_span_len(),
            Dither::TPDF(d) => d.current_span_len(),
        }
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        match self {
            Dither::GPDF(d) => d.channels(),
            Dither::HighPass(d) => d.channels(),
            Dither::RPDF(d) => d.channels(),
            Dither::TPDF(d) => d.channels(),
        }
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        match self {
            Dither::GPDF(d) => d.sample_rate(),
            Dither::HighPass(d) => d.sample_rate(),
            Dither::RPDF(d) => d.sample_rate(),
            Dither::TPDF(d) => d.sample_rate(),
        }
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        match self {
            Dither::GPDF(d) => d.total_duration(),
            Dither::HighPass(d) => d.total_duration(),
            Dither::RPDF(d) => d.total_duration(),
            Dither::TPDF(d) => d.total_duration(),
        }
    }

    #[inline]
    fn bits_per_sample(&self) -> Option<BitDepth> {
        match self {
            Dither::GPDF(d) => d.bits_per_sample(),
            Dither::HighPass(d) => d.bits_per_sample(),
            Dither::RPDF(d) => d.bits_per_sample(),
            Dither::TPDF(d) => d.bits_per_sample(),
        }
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), crate::source::SeekError> {
        match self {
            Dither::GPDF(d) => d.try_seek(pos),
            Dither::HighPass(d) => d.try_seek(pos),
            Dither::RPDF(d) => d.try_seek(pos),
            Dither::TPDF(d) => d.try_seek(pos),
        }
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
    fn test_dither_algorithms() {
        let source = SineWave::new(440.0).take_duration(std::time::Duration::from_millis(10));

        // Test all four algorithms
        let mut gpdf = Dither::new(source.clone(), TEST_BIT_DEPTH, Algorithm::GPDF);
        let mut highpass = Dither::new(source.clone(), TEST_BIT_DEPTH, Algorithm::HighPass);
        let mut rpdf = Dither::new(source.clone(), TEST_BIT_DEPTH, Algorithm::RPDF);
        let mut tpdf = Dither::new(source, TEST_BIT_DEPTH, Algorithm::TPDF);

        for _ in 0..10 {
            let gpdf_sample = gpdf.next().unwrap();
            let highpass_sample = highpass.next().unwrap();
            let rpdf_sample = rpdf.next().unwrap();
            let tpdf_sample = tpdf.next().unwrap();

            // RPDF and TPDF should be bounded
            assert!((-1.0..=1.0).contains(&rpdf_sample));
            assert!((-1.0..=1.0).contains(&tpdf_sample));

            // Note: GPDF (Gaussian) and HighPass (Blue) may occasionally exceed [-1,1] bounds
            assert!(gpdf_sample.is_normal());
            assert!(highpass_sample.is_normal());
        }
    }

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
