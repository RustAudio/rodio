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

use crate::{BitDepth, ChannelCount, Sample, SampleRate, Source};
use std::time::Duration;

impl<I, N> Iterator for Dither<I, N>
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

impl<I, N> Source for Dither<I, N>
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
    fn try_seek(&mut self, pos: Duration) -> Result<(), crate::source::SeekError> {
        self.input.try_seek(pos)
    }
}

#[derive(Clone, Debug)]
pub struct Dither<I, N> {
    input: I,
    noise: N,
    lsb_amplitude: f32,
}

trait DitherAlgorithm {
    type Noise;
    fn build_noise(self, sample_rate: SampleRate) -> Self::Noise;
}

macro_rules! dither_algos {
    ($($(#[$outer:meta])* $name:ident, $noise:ident);+) => {
        $(
            $(#[$outer])*
            struct $name;

            impl DitherAlgorithm for $name {
                type Noise = crate::source::noise::$noise;
                fn build_noise(self, sample_rate: SampleRate) -> Self::Noise {
                    crate::source::noise::$noise::new(sample_rate)
                }
            }
        )+
    };
}
dither_algos! {
    /// GPDF (Gaussian PDF) - normal/bell curve distribution.
    ///
    /// Uses Gaussian white noise which more closely mimics natural processes and
    /// analog circuits. Higher noise floor than TPDF.
    GPDF, WhiteGaussian;
    /// High-pass dithering - reduces low-frequency artifacts.
    ///
    /// Uses blue noise (high-pass filtered white noise) to push dither energy
    /// toward higher frequencies. Particularly effective for reducing audible
    /// low-frequency modulation artifacts. Best for material with significant
    /// low-frequency content where traditional white dither might be audible.
    HighPass, Blue;
    /// RPDF (Rectangular PDF) - uniform distribution.
    ///
    /// Uses uniform white noise for basic decorrelation. Simpler than TPDF but
    /// allows some correlation between signal and quantization error at low levels.
    /// Slightly lower noise floor than TPDF.
    RPDF, WhiteUniform;
    /// TPDF (Triangular PDF) - triangular distribution.
    ///
    /// The gold standard for audio dithering. Provides mathematically optimal
    /// decorrelation by completely eliminating correlation between the original
    /// signal and quantization error.
    TPDF, WhiteTriangular
}

fn dither<I, A>(
    input: I,
    algo: A,
    target_bits: BitDepth,
) -> Dither<I, <A as DitherAlgorithm>::Noise>
where
    I: Source,
    A: DitherAlgorithm,
{
    // LSB amplitude for signed audio: 1.0 / (2^(bits-1))
    // This represents the amplitude of one quantization level
    let lsb_amplitude = if target_bits.get() >= Sample::MANTISSA_DIGITS {
        // For bit depths at or beyond the floating point precision limit,
        // the LSB amplitude calculation becomes meaningless
        Sample::MIN_POSITIVE
    } else {
        1.0 / (1_i64 << (target_bits.get() - 1)) as f32
    };

    Dither {
        noise: algo.build_noise(input.sample_rate()),
        input,
        lsb_amplitude,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{SineWave, Source};
    use crate::{nz, BitDepth, SampleRate};

    fn show_api() {
        let source = SineWave::new(440.0).take_duration(std::time::Duration::from_millis(10));
        let source = dither(source, GPDF, nz!(16));
    }
    //     const TEST_SAMPLE_RATE: SampleRate = nz!(44100);
    //     const TEST_BIT_DEPTH: BitDepth = nz!(16);
    //
    //     #[test]
    //     fn test_dither_algorithms() {
    //         let source = SineWave::new(440.0).take_duration(std::time::Duration::from_millis(10));
    //
    //         // Test all four algorithms
    //         let mut gpdf = Dither::new(source.clone(), TEST_BIT_DEPTH, Algorithm::GPDF);
    //         let mut highpass = Dither::new(source.clone(), TEST_BIT_DEPTH, Algorithm::HighPass);
    //         let mut rpdf = Dither::new(source.clone(), TEST_BIT_DEPTH, Algorithm::RPDF);
    //         let mut tpdf = Dither::new(source, TEST_BIT_DEPTH, Algorithm::TPDF);
    //
    //         for _ in 0..10 {
    //             let gpdf_sample = gpdf.next().unwrap();
    //             let highpass_sample = highpass.next().unwrap();
    //             let rpdf_sample = rpdf.next().unwrap();
    //             let tpdf_sample = tpdf.next().unwrap();
    //
    //             // RPDF and TPDF should be bounded
    //             assert!((-1.0..=1.0).contains(&rpdf_sample));
    //             assert!((-1.0..=1.0).contains(&tpdf_sample));
    //
    //             // Note: GPDF (Gaussian) and HighPass (Blue) may occasionally exceed [-1,1] bounds
    //             assert!(gpdf_sample.is_normal());
    //             assert!(highpass_sample.is_normal());
    //         }
    //     }
    //
    //     #[test]
    //     fn test_dither_adds_noise() {
    //         let source = SineWave::new(440.0).take_duration(std::time::Duration::from_millis(10));
    //         let mut dithered = Dither::new(source.clone(), TEST_BIT_DEPTH, Algorithm::TPDF);
    //         let mut undithered = source;
    //
    //         // Collect samples from both sources
    //         let dithered_samples: Vec<f32> = (0..10).filter_map(|_| dithered.next()).collect();
    //         let undithered_samples: Vec<f32> = (0..10).filter_map(|_| undithered.next()).collect();
    //
    //         let lsb = 1.0 / (1_i64 << (TEST_BIT_DEPTH.get() - 1)) as f32;
    //
    //         // Verify dithered samples differ from undithered and are reasonable
    //         for (i, (&dithered_sample, &undithered_sample)) in dithered_samples
    //             .iter()
    //             .zip(undithered_samples.iter())
    //             .enumerate()
    //         {
    //             // Should be finite
    //             assert!(
    //                 dithered_sample.is_finite(),
    //                 "Dithered sample {} should be finite",
    //                 i
    //             );
    //
    //             // The difference should be small (just dither noise)
    //             let diff = (dithered_sample - undithered_sample).abs();
    //             let max_expected_diff = lsb * 2.0; // Max triangular dither amplitude
    //             assert!(
    //                 diff <= max_expected_diff,
    //                 "Dither noise too large: sample {}, diff {}, max expected {}",
    //                 i,
    //                 diff,
    //                 max_expected_diff
    //             );
    //         }
    //     }
}
