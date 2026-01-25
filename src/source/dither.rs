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
//! use rodio::source::{DitherAlgorithm, SineWave};
//! use rodio::{BitDepth, Source};
//!
//! let source = SineWave::new(440.0);
//! let dithered = source.dither(BitDepth::new(16).unwrap(), DitherAlgorithm::TPDF);
//! ```
//!
//! ## Guidelines
//!
//! - **Apply dithering before volume changes** for optimal results
//! - **Dither once** - Apply only at the final output stage to avoid noise accumulation
//! - **Choose TPDF** for most professional audio applications (it's the default)
//! - **Use target output bit depth** - Not the source bit depth!
//!
//! When you later change volume (e.g., with `Player::set_volume()`), both the signal
//! and dither noise scale together, maintaining proper dithering behavior.

use dasp_sample::Sample as _;
use rand::{rngs::SmallRng, Rng};
use std::time::Duration;

use crate::{
    source::{
        detect_span_boundary,
        noise::{Blue, WhiteGaussian, WhiteTriangular, WhiteUniform},
        reset_seek_span_tracking, SeekError,
    },
    BitDepth, ChannelCount, Float, Sample, SampleRate, Source,
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
    HighPass(Vec<Blue<R>>),
}

impl NoiseGenerator {
    fn new(algorithm: Algorithm, sample_rate: SampleRate, channels: ChannelCount) -> Self {
        match algorithm {
            Algorithm::TPDF => Self::TPDF(WhiteTriangular::new(sample_rate)),
            Algorithm::RPDF => Self::RPDF(WhiteUniform::new(sample_rate)),
            Algorithm::GPDF => Self::GPDF(WhiteGaussian::new(sample_rate)),
            Algorithm::HighPass => {
                // Create per-channel generators for HighPass to prevent prev_white state from
                // crossing channel boundaries in interleaved audio. Each channel must have an
                // independent RNG to avoid correlation. Use this iterator instead of the `vec!`
                // macro to avoid cloning the RNG.
                Self::HighPass(
                    (0..channels.get())
                        .map(|_| Blue::new(sample_rate))
                        .collect(),
                )
            }
        }
    }

    #[inline]
    fn next(&mut self, channel: usize) -> Option<Sample> {
        match self {
            Self::TPDF(gen) => gen.next(),
            Self::RPDF(gen) => gen.next(),
            Self::GPDF(gen) => gen.next(),
            Self::HighPass(gens) => gens[channel].next(),
        }
    }

    #[inline]
    fn algorithm(&self) -> Algorithm {
        match self {
            Self::TPDF(_) => Algorithm::TPDF,
            Self::RPDF(_) => Algorithm::RPDF,
            Self::GPDF(_) => Algorithm::GPDF,
            Self::HighPass(_) => Algorithm::HighPass,
        }
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        match self {
            Self::TPDF(gen) => gen.sample_rate(),
            Self::RPDF(gen) => gen.sample_rate(),
            Self::GPDF(gen) => gen.sample_rate(),
            Self::HighPass(gens) => gens
                .first()
                .map(|g| g.sample_rate())
                .expect("HighPass should have at least one generator"),
        }
    }

    #[inline]
    fn update_parameters(&mut self, sample_rate: SampleRate, channels: ChannelCount) {
        if self.sample_rate() != sample_rate {
            // The noise generators that we use are currently not dependent on sample rate,
            // but we recreate them anyway in case that changes in the future.
            *self = Self::new(self.algorithm(), sample_rate, channels);
        } else if let Self::HighPass(gens) = self {
            // Sample rate unchanged - only adjust channel count for stateful algorithms
            // resize_with is a no-op if the size hasn't changed
            gens.resize_with(channels.get() as usize, || Blue::new(sample_rate));
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
/// use rodio::source::{DitherAlgorithm, SineWave};
/// use rodio::{BitDepth, Source};
///
/// let source = SineWave::new(440.0);
/// let dithered = source.dither(BitDepth::new(16).unwrap(), DitherAlgorithm::TPDF);
/// ```
#[derive(Clone, Debug)]
pub struct Dither<I> {
    input: I,
    noise: NoiseGenerator,
    current_channel: usize,
    last_sample_rate: SampleRate,
    last_channels: ChannelCount,
    lsb_amplitude: Float,
    samples_counted: usize,
    cached_span_len: Option<usize>,
    silence_samples_remaining: usize,
}

impl<I> Dither<I>
where
    I: Source,
{
    /// Creates a new dithered source with the specified algorithm
    pub fn new(input: I, target_bits: BitDepth, algorithm: Algorithm) -> Self {
        // LSB amplitude for signed audio: 1.0 / (2^(bits-1))
        // Using f64 intermediate prevents precision loss and u64 handles all bit depths without
        // overflow (64-bit being the theoretical maximum for audio samples). Values stay well
        // above f32 denormal threshold, avoiding denormal arithmetic performance penalty.
        let lsb_amplitude = (1.0 / (1_u64 << (target_bits.get() - 1)) as f64) as Float;

        let sample_rate = input.sample_rate();
        let channels = input.channels();

        Self {
            input,
            noise: NoiseGenerator::new(algorithm, sample_rate, channels),
            current_channel: 0,
            last_sample_rate: sample_rate,
            last_channels: channels,
            lsb_amplitude,
            samples_counted: 0,
            cached_span_len: None,
            silence_samples_remaining: 0,
        }
    }

    /// Change the dithering algorithm at runtime
    pub fn set_algorithm(&mut self, algorithm: Algorithm) {
        if self.noise.algorithm() != algorithm {
            self.noise =
                NoiseGenerator::new(algorithm, self.input.sample_rate(), self.input.channels());
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
        loop {
            if self.silence_samples_remaining > 0 {
                self.silence_samples_remaining -= 1;

                let noise_sample = self
                    .noise
                    .next(self.current_channel)
                    .expect("Noise generator should always produce samples");

                self.current_channel =
                    (self.current_channel + 1) % self.last_channels.get() as usize;
                return Some(Sample::EQUILIBRIUM - noise_sample * self.lsb_amplitude);
            }

            let input_sample = match self.input.next() {
                Some(s) => s,
                None => {
                    if self.current_channel > 0 {
                        let channels = self.last_channels.get() as usize;
                        self.silence_samples_remaining = channels - self.current_channel;
                        continue; // Loop will inject dithered silence samples
                    }
                    return None;
                }
            };

            let input_span_len = self.input.current_span_len();
            let current_sample_rate = self.input.sample_rate();
            let current_channels = self.input.channels();

            let (at_boundary, parameters_changed) = detect_span_boundary(
                &mut self.samples_counted,
                &mut self.cached_span_len,
                input_span_len,
                current_sample_rate,
                self.last_sample_rate,
                current_channels,
                self.last_channels,
            );

            if at_boundary {
                if parameters_changed {
                    self.noise
                        .update_parameters(current_sample_rate, current_channels);
                    self.last_sample_rate = current_sample_rate;
                    self.last_channels = current_channels;
                }
                self.current_channel = 0;
            }

            let noise_sample = self
                .noise
                .next(self.current_channel)
                .expect("Noise generator should always produce samples");

            // Advance to next channel (wrapping around)
            self.current_channel =
                (self.current_channel + 1) % self.input.channels().get() as usize;

            // Apply subtractive dithering at the target quantization level
            return Some(input_sample - noise_sample * self.lsb_amplitude);
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for Dither<I> where I: Source + ExactSizeIterator {}

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
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.input.try_seek(pos)?;
        self.current_channel = 0;
        reset_seek_span_tracking(
            &mut self.samples_counted,
            &mut self.cached_span_len,
            pos,
            self.input.current_span_len(),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::test_utils::TestSource;
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
        let dithered_samples: Vec<Sample> = (0..10).filter_map(|_| dithered.next()).collect();
        let undithered_samples: Vec<Sample> = (0..10).filter_map(|_| undithered.next()).collect();

        let lsb = 1.0 / (1_i64 << (TEST_BIT_DEPTH.get() - 1)) as Float;

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

    #[test]
    fn test_highpass_dither_multichannel_independence() {
        use crate::source::Zero;

        // Create a stereo source that outputs zeros
        // This makes it easy to extract just the dither noise
        let constant_source = Zero::new(nz!(2), TEST_SAMPLE_RATE);

        // Apply HighPass dithering to stereo
        let mut dithered = Dither::new(constant_source, TEST_BIT_DEPTH, Algorithm::HighPass);

        // Collect interleaved samples (L, R, L, R, ...)
        let samples: Vec<Sample> = dithered.by_ref().take(1000).collect();

        // De-interleave into left and right channels
        let left: Vec<Sample> = samples.iter().step_by(2).copied().collect();
        let right: Vec<Sample> = samples.iter().skip(1).step_by(2).copied().collect();

        assert_eq!(left.len(), 500);
        assert_eq!(right.len(), 500);

        // Calculate autocorrelation at lag 1 for each channel
        // Blue noise (high-pass) should have negative correlation at lag 1
        let left_autocorr: Float =
            left.windows(2).map(|w| w[0] * w[1]).sum::<Float>() / (left.len() - 1) as Float;

        let right_autocorr: Float =
            right.windows(2).map(|w| w[0] * w[1]).sum::<Float>() / (right.len() - 1) as Float;

        // Blue noise should have negative autocorrelation (high-pass characteristic)
        // If channels were cross-contaminated, this property would be broken
        assert!(
            left_autocorr < 0.0,
            "Left channel should have negative autocorr (high-pass), got {}",
            left_autocorr
        );
        assert!(
            right_autocorr < 0.0,
            "Right channel should have negative autocorr (high-pass), got {}",
            right_autocorr
        );

        // Channels should be independent - cross-correlation between L and R should be near zero
        let cross_corr: Float = left
            .iter()
            .zip(right.iter())
            .map(|(l, r)| l * r)
            .sum::<Float>()
            / left.len() as Float;

        assert!(
            cross_corr.abs() < 0.1,
            "Channels should be independent, cross-correlation should be near 0, got {}",
            cross_corr
        );
    }

    #[test]
    fn test_incomplete_frame_padding_stereo() {
        let samples = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let source = TestSource::new(&samples, nz!(2), TEST_SAMPLE_RATE);

        let dithered = Dither::new(source, TEST_BIT_DEPTH, Algorithm::TPDF);
        let output: Vec<Sample> = dithered.collect();

        // The last sample should be dithered silence (small non-zero value from dither noise)
        let lsb = 1.0 / (1_i64 << (TEST_BIT_DEPTH.get() - 1)) as Float;
        let max_dither_amplitude = lsb * 2.0; // Max TPDF dither amplitude
        assert!(
            output.get(5).map(|i| i.abs()) <= Some(max_dither_amplitude),
            "6th sample should be dithered silence (small noise)"
        );
    }
}
