//! Audio resampling from one sample rate to another.
//!
//! # Quick Start
//!
//! Use the [`Source::resample`] method with a quality preset:
//!
//! ```rust
//! use rodio::SampleRate;
//! use rodio::source::{SineWave, Source, ResampleConfig};
//!
//! let source = SineWave::new(440.0);
//! let config = ResampleConfig::balanced();
//! let resampled = source.resample(SampleRate::new(96000).unwrap(), config);
//! ```
//!
//! For advanced control, use the [`ResampleConfig`] builder:
//!
//! ```rust
//! use rodio::math::nz;
//! use rodio::source::{SineWave, Source, Resample, ResampleConfig};
//! use rodio::source::resample::{Sinc, WindowFunction};
//!
//! let source = SineWave::new(440.0);
//! let config = ResampleConfig::sinc()                  // Sinc resampling
//!     .sinc_len(nz!(256))                              // 256-tap filter
//!     .interpolation(Sinc::Cubic)                      // Cubic interpolation
//!     .window(WindowFunction::BlackmanHarris2)         // Squared Blackman-Harris window
//!     .chunk_size(nz!(512))                            // Low latency (5.3 ms @ 1-channel 96 kHz)
//!     .build();
//! let resampled = Resample::new(source, nz!(96000), config);
//! ```
//!
//! # Understanding Resampling
//!
//! ## Polynomial vs. Sinc Interpolation
//!
//! When converting between sample rates, sample values at positions that don't exist in the
//! original signal need to be calculated. There are two main approaches:
//!
//! **Polynomial interpolation** is fast but does not include anti-aliasing. This can cause
//! artifacts in the output audio. Higher degrees provide smoother interpolation but cannot
//! prevent these artifacts.
//!
//! **Sinc interpolation** uses a windowed sinc function for mathematically correct reconstruction.
//! It is of higher quality and includes anti-aliasing to reduce artifacts, but is more
//! computationally expensive.
//!
//! ## Fixed vs Arbitrary Ratios
//!
//! A **fixed ratio** is when the sample rate conversion can be expressed as a simple fraction,
//! like 1:2 (e.g., 48 kHz and 96 kHz) or 147:160 (e.g., 44.1 kHz and 48 kHz).
//!
//! When the resampler is configured for sinc interpolation, it automatically detects these ratios
//! and optimizes resampling by switching to:
//! 1. optimized FFT-based processing when the `rubato-fft` feature is enabled
//! 2. sinc interpolation with nearest-neighbor lookup when FFT is not available
//!
//! This reduces CPU usage while providing highest quality.
//!
//! **Arbitrary ratios** (non-reducible or large fractions) use the async sinc resampler, which
//! can handle any conversion. This is CPU intensive and should be compiled with release profile to
//! prevent choppy audio.
//!
//! # Quality Presets
//!
//! As per [`CamillaDSP`](https://henquist.github.io/3.0.x/):
//!
//! | Parameter | [`VeryFast`](ResampleConfig::very_fast) | [`Fast`](ResampleConfig::fast) | [`Balanced`](ResampleConfig::balanced) | [`Accurate`](ResampleConfig::accurate) |
//! | sinc_len | 64 | 128 | 192 | 256 |
//! | oversampling_factor | 1024 | 1024 | 512 | 256 |
//! | interpolation | Linear | Linear | Quadratic | Cubic |
//! | window | Hann2 | Blackman2 | BlackmanHarris2 | BlackmanHarris2 |
//! | f_cutoff (#) | 0.91 | 0.92 | 0.93 | 0.95 |
//! (#) These cutoff values are approximate. The actual values used are calculated automatically at runtime for the combination of sinc length and window.

#![cfg_attr(docsrs, feature(doc_cfg))]

use std::time::Duration;

use num_rational::Ratio;
use ::rubato::Resampler as _;

use super::{reset_seek_span_tracking, SeekError};
use crate::{
    common::{ChannelCount, Sample, SampleRate},
    Float, Source,
};

mod builder;
mod rubato;

use rubato::{ResampleInner, RubatoAsyncResample};
#[cfg(feature = "rubato-fft")]
use rubato::RubatoFftResample;

pub use builder::{
    Poly, PolyConfigBuilder, ResampleConfig, Sinc, SincConfigBuilder, WindowFunction,
};

/// Maximum for optimized fixed-ratio resampling: 44.1 and 384 kHz (147:1280).
const MAX_FIXED_RATIO: u32 = 1280;

/// Resamples an audio source to a target sample rate using Rubato.
#[derive(Debug)]
pub struct Resample<I>
where
    I: Source,
{
    inner: Option<ResampleInner<I>>,
    target_rate: SampleRate,
    config: ResampleConfig,
    cached_input_span_len: Option<usize>,
}

impl<I> Clone for Resample<I>
where
    I: Source + Clone,
{
    fn clone(&self) -> Self {
        // Shallow clone: this resets filter state
        let source = self.inner().clone();
        Resample::new(source, self.target_rate, self.config.clone())
    }
}

impl<I> Resample<I>
where
    I: Source,
{
    /// Create a new resampler with the given configuration.
    pub fn new(source: I, target_rate: SampleRate, config: ResampleConfig) -> Self {
        let inner = Self::create_resampler(source, target_rate, &config);
        let cached_input_span_len = match &inner {
            ResampleInner::Passthrough { .. } => inner.input().current_span_len(),
            ResampleInner::Poly(resampler) => resampler.input.current_span_len(),
            ResampleInner::Sinc(resampler) => resampler.input.current_span_len(),
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => resampler.input.current_span_len(),
        };

        Self {
            inner: Some(inner),
            target_rate,
            config,
            cached_input_span_len,
        }
    }

    /// Helper method to create a resampler from a source using the stored config and target rate.
    fn create_resampler(
        source: I,
        target_rate: SampleRate,
        config: &ResampleConfig,
    ) -> ResampleInner<I> {
        let source_rate = source.sample_rate();

        if source.is_exhausted() || source_rate == target_rate {
            let channels = source.channels();
            ResampleInner::Passthrough {
                source,
                input_span_pos: 0,
                channels,
                source_rate,
            }
        } else {
            let ratio = Ratio::new(target_rate.get(), source_rate.get());
            match config {
                ResampleConfig::Poly { degree, chunk_size } => {
                    let resampler =
                        RubatoAsyncResample::new_poly(source, target_rate, *chunk_size, *degree)
                            .expect("Failed to create polynomial resampler");
                    ResampleInner::Poly(resampler)
                }
                #[cfg(feature = "rubato-fft")]
                ResampleConfig::Sinc {
                    sinc_len,
                    oversampling_factor,
                    interpolation,
                    window,
                    f_cutoff,
                    chunk_size,
                    sub_chunks,
                } => {
                    if *ratio.numer() <= MAX_FIXED_RATIO && *ratio.denom() <= MAX_FIXED_RATIO {
                        // Use FFT resampler for optimal performance
                        let resampler =
                            RubatoFftResample::new(source, target_rate, *chunk_size, *sub_chunks)
                                .expect("Failed to create FFT resampler");
                        ResampleInner::Fft(resampler)
                    } else {
                        let resampler = RubatoAsyncResample::new_sinc(
                            source,
                            target_rate,
                            *chunk_size,
                            *sinc_len,
                            *f_cutoff,
                            *oversampling_factor,
                            *interpolation,
                            *window,
                        )
                        .expect("Failed to create sinc resampler");
                        ResampleInner::Sinc(resampler)
                    }
                }
                #[cfg(not(feature = "rubato-fft"))]
                ResampleConfig::Sinc {
                    sinc_len,
                    oversampling_factor,
                    interpolation,
                    window,
                    f_cutoff,
                    chunk_size,
                } => {
                    if *ratio.numer() <= MAX_FIXED_RATIO && *ratio.denom() <= MAX_FIXED_RATIO {
                        // Fixed ratio without FFT - use Sinc::Nearest optimization
                        // Set oversampling_factor to match the ratio for optimal performance
                        let ratio = *ratio.numer().max(ratio.denom()) as usize;
                        let resampler = RubatoAsyncResample::new_sinc(
                            source,
                            target_rate,
                            *chunk_size,
                            *sinc_len,
                            *f_cutoff,
                            ratio,
                            Sinc::Nearest,
                            *window,
                        )
                        .expect("Failed to create optimized sinc resampler");
                        ResampleInner::Sinc(resampler)
                    } else {
                        let resampler = RubatoAsyncResample::new_sinc(
                            source,
                            target_rate,
                            *chunk_size,
                            *sinc_len,
                            *f_cutoff,
                            *oversampling_factor,
                            *interpolation,
                            *window,
                        )
                        .expect("Failed to create sinc resampler");
                        ResampleInner::Sinc(resampler)
                    }
                }
            }
        }
    }

    /// Returns a reference to the inner source.
    #[inline]
    pub fn inner(&self) -> &I {
        match self.inner.as_ref().unwrap() {
            ResampleInner::Passthrough { source, .. } => source,
            ResampleInner::Poly(resampler) => &resampler.input,
            ResampleInner::Sinc(resampler) => &resampler.input,
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => &resampler.input,
        }
    }

    /// Returns a mutable reference to the inner source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        match self.inner.as_mut().unwrap() {
            ResampleInner::Passthrough { source, .. } => source,
            ResampleInner::Poly(resampler) => &mut resampler.input,
            ResampleInner::Sinc(resampler) => &mut resampler.input,
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => &mut resampler.input,
        }
    }

    /// Returns the inner source.
    #[inline]
    pub fn into_inner(self) -> I {
        self.inner.unwrap().into_inner()
    }
}

impl<I> Source for Resample<I>
where
    I: Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        let (
            input_span_len,
            input_sample_rate,
            input_exhausted,
            output_buffer_len,
            output_buffer_pos,
            output_frames_next,
        ) = match self.inner.as_ref().unwrap() {
            ResampleInner::Passthrough { source, .. } => return source.current_span_len(),
            ResampleInner::Poly(resampler) | ResampleInner::Sinc(resampler) => (
                resampler.input.current_span_len(),
                resampler.input.sample_rate(),
                resampler.input.is_exhausted(),
                resampler.output_buffer_len,
                resampler.output_buffer_pos,
                resampler.resampler.output_frames_next(),
            ),
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => (
                resampler.input.current_span_len(),
                resampler.input.sample_rate(),
                resampler.input.is_exhausted(),
                resampler.output_buffer_len,
                resampler.output_buffer_pos,
                resampler.resampler.output_frames_next(),
            ),
        };

        let ratio = Ratio::new(self.sample_rate().get(), input_sample_rate.get());
        if ratio.is_integer() {
            // Integer upsampling (2x, 3x, etc.) - always exact and frame-aligned
            input_span_len.map(|len| *ratio.numer() as usize * len)
        } else {
            // When the ratio contains a fraction, we cannot choose the floor or ceiling
            // arbitrarily, because the resampler may produce either based on its internal state
            if output_buffer_pos < output_buffer_len {
                // Running state: we are iterating over our buffer with resampled samples
                Some(output_buffer_len)
            } else if input_exhausted {
                // End state: we are at the end of our buffer and the source is exhausted
                Some(0)
            } else {
                // Initial state: our buffer is empty until the first call to next() loads it with
                // resampled samples. Return the size of the next buffer.
                Some(output_frames_next * self.channels().get() as usize)
            }
        }
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.target_rate
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        match self.inner.as_ref().unwrap() {
            ResampleInner::Passthrough { source, .. } => source.channels(),
            ResampleInner::Poly(resampler) => resampler.channels,
            ResampleInner::Sinc(resampler) => resampler.channels,
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => resampler.channels,
        }
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        match self.inner.as_ref().unwrap() {
            ResampleInner::Passthrough { source, .. } => source.total_duration(),
            ResampleInner::Poly(resampler) => resampler.input.total_duration(),
            ResampleInner::Sinc(resampler) => resampler.input.total_duration(),
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => resampler.input.total_duration(),
        }
    }

    #[inline]
    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        match self.inner.as_mut().unwrap() {
            ResampleInner::Passthrough { source, .. } => source.try_seek(position)?,
            ResampleInner::Poly(r) | ResampleInner::Sinc(r) => {
                r.input.try_seek(position)?;
                r.reset();
            }
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(r) => {
                r.input.try_seek(position)?;
                r.reset();
            }
        }

        let input_span_len = self.inner.as_ref().unwrap().input().current_span_len();

        match self.inner.as_mut().unwrap() {
            ResampleInner::Passthrough {
                input_span_pos: input_samples_consumed,
                ..
            } => {
                reset_seek_span_tracking(
                    input_samples_consumed,
                    &mut self.cached_input_span_len,
                    position,
                    input_span_len,
                );
            }
            ResampleInner::Poly(r) | ResampleInner::Sinc(r) => {
                reset_seek_span_tracking(
                    &mut r.input_samples_consumed,
                    &mut self.cached_input_span_len,
                    position,
                    input_span_len,
                );
            }
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(r) => {
                reset_seek_span_tracking(
                    &mut r.input_samples_consumed,
                    &mut self.cached_input_span_len,
                    position,
                    input_span_len,
                );
            }
        }

        Ok(())
    }
}

impl<I> Iterator for Resample<I>
where
    I: Source,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let sample = match self.inner.as_mut().unwrap() {
            ResampleInner::Passthrough { source, .. } => source.next()?,
            ResampleInner::Poly(resampler) => resampler.next_sample()?,
            ResampleInner::Sinc(resampler) => resampler.next_sample()?,
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => resampler.next_sample()?,
        };

        // If input reports no span length, parameters are stable by contract
        let input_span_len = self.inner.as_ref().unwrap().input().current_span_len();
        if input_span_len.is_some() {
            let (expected_channels, expected_rate, samples_consumed) =
                match self.inner.as_mut().unwrap() {
                    ResampleInner::Passthrough {
                        input_span_pos: input_samples_consumed,
                        channels,
                        source_rate,
                        ..
                    } => {
                        *input_samples_consumed += 1;
                        (*channels, *source_rate, *input_samples_consumed)
                    }
                    ResampleInner::Poly(r) | ResampleInner::Sinc(r) => {
                        (r.channels, r.source_rate, r.input_samples_consumed)
                    }
                    #[cfg(feature = "rubato-fft")]
                    ResampleInner::Fft(r) => (r.channels, r.source_rate, r.input_samples_consumed),
                };

            // Get current parameters from input
            let input = self.inner.as_ref().unwrap().input();
            let current_channels = input.channels();
            let current_rate = input.sample_rate();

            // Determine if we're at a span boundary:
            // - Counting mode (Some): boundary when we've consumed span_len samples
            // - Detection mode (None): boundary when parameters change (mid-span seek recovery)
            let mut parameters_changed = false;
            let at_boundary = {
                let known_boundary = self
                    .cached_input_span_len
                    .map(|cached_len| samples_consumed >= cached_len);

                // In counting mode: only check parameters at boundary
                // In detection mode: check parameters at every sample until detecting a boundary
                if known_boundary.is_none_or(|at_boundary| at_boundary) {
                    parameters_changed =
                        current_channels != expected_channels || current_rate != expected_rate;
                }

                known_boundary.unwrap_or(parameters_changed)
            };

            if at_boundary {
                // Update cached span length (exits detection mode if we were in it)
                self.cached_input_span_len = input_span_len;

                if parameters_changed {
                    // Recreate resampler - new resampler will have counters reset to 0
                    let source = self.inner.take().unwrap().into_inner();
                    self.inner = Some(Self::create_resampler(
                        source,
                        self.target_rate,
                        &self.config,
                    ));
                } else {
                    // Just crossed boundary without parameter change, reset counter
                    match self.inner.as_mut().unwrap() {
                        ResampleInner::Passthrough {
                            input_span_pos: input_samples_consumed,
                            ..
                        } => {
                            *input_samples_consumed = 0;
                        }
                        ResampleInner::Poly(r) | ResampleInner::Sinc(r) => {
                            r.input_samples_consumed = 0;
                        }
                        #[cfg(feature = "rubato-fft")]
                        ResampleInner::Fft(r) => {
                            r.input_samples_consumed = 0;
                        }
                    }
                }
            }
        }

        Some(sample)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (input_hint, source_rate, buffered_remaining) = match self.inner.as_ref().unwrap() {
            ResampleInner::Passthrough { source, .. } => return source.size_hint(),
            ResampleInner::Poly(resampler) | ResampleInner::Sinc(resampler) => {
                let input_hint = resampler.input.size_hint();
                let buffered_remaining = resampler.output_buffer_len - resampler.output_buffer_pos;
                (input_hint, resampler.source_rate, buffered_remaining)
            }
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => {
                let input_hint = resampler.input.size_hint();
                let buffered_remaining = resampler.output_buffer_len - resampler.output_buffer_pos;
                (input_hint, resampler.source_rate, buffered_remaining)
            }
        };

        let (input_lower, input_upper) = input_hint;
        let ratio = self.target_rate.get() as Float / source_rate.get() as Float;

        let lower = buffered_remaining + (input_lower as Float * ratio).ceil() as usize;
        let upper =
            input_upper.map(|upper| buffered_remaining + (upper as Float * ratio).ceil() as usize);

        (lower, upper)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{from_iter, SineWave};
    use crate::Source;
    use dasp_sample::ToSample;
    use quickcheck::{quickcheck, Arbitrary, Gen, TestResult};
    use std::num::NonZero;

    #[derive(Debug, Clone, Copy)]
    struct TestSampleRate(SampleRate);

    impl Arbitrary for TestSampleRate {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate realistic sample rates: 8 kHz to 384 kHz
            let rate = u32::arbitrary(g) % 376_001 + 8_000;
            TestSampleRate(SampleRate::new(rate).unwrap())
        }
    }

    impl std::ops::Deref for TestSampleRate {
        type Target = SampleRate;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct TestChannelCount(ChannelCount);

    impl Arbitrary for TestChannelCount {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate realistic channel counts: 1 to 8
            let channels = (u16::arbitrary(g) % 7) + 1;
            TestChannelCount(ChannelCount::new(channels).unwrap())
        }
    }

    impl std::ops::Deref for TestChannelCount {
        type Target = ChannelCount;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    struct TestSource {
        samples: Vec<Sample>,
        index: usize,
        sample_rate: SampleRate,
        channels: ChannelCount,
    }

    impl TestSource {
        fn new(samples: Vec<Sample>, sample_rate: SampleRate, channels: ChannelCount) -> Self {
            Self {
                samples,
                index: 0,
                sample_rate,
                channels,
            }
        }
    }

    impl Iterator for TestSource {
        type Item = Sample;

        fn next(&mut self) -> Option<Self::Item> {
            if self.index < self.samples.len() {
                let sample = self.samples[self.index];
                self.index += 1;
                Some(sample)
            } else {
                None
            }
        }
    }

    impl Source for TestSource {
        fn current_span_len(&self) -> Option<usize> {
            Some(self.samples.len())
        }

        fn sample_rate(&self) -> SampleRate {
            self.sample_rate
        }

        fn channels(&self) -> ChannelCount {
            self.channels
        }

        fn total_duration(&self) -> Option<Duration> {
            let samples = self.samples.len() / self.channels.get() as usize;
            Some(Duration::from_secs_f64(
                samples as f64 / self.sample_rate.get() as f64,
            ))
        }

        fn try_seek(&mut self, _position: Duration) -> Result<(), SeekError> {
            Ok(())
        }
    }

    /// Convert and truncate input to contain a frame-aligned number of samples.
    fn convert_to_frames<S: dasp_sample::Sample + ToSample<crate::Sample>>(
        input: Vec<S>,
        channels: ChannelCount,
    ) -> Vec<Sample> {
        let mut input: Vec<Sample> = input.iter().map(|x| x.to_sample()).collect();
        let frame_size = channels.get() as usize;
        input.truncate(frame_size * (input.len() / frame_size));
        input
    }

    quickcheck! {
        /// Check that resampling an empty input produces no output.
        fn empty(from: TestSampleRate, to: TestSampleRate, channels: TestChannelCount) -> bool {
            let input = vec![];
            let config = ResampleConfig::default();
            let source = from_iter(input.clone().into_iter(), *channels, *from);
            let output = Resample::new(source, *to, config).collect::<Vec<_>>();
            input == output
        }

        /// Check that resampling to the same rate does not change the signal.
        fn identity(from: TestSampleRate, channels: TestChannelCount, input: Vec<i16>) -> bool {
            let input = convert_to_frames(input, *channels);
            let config = ResampleConfig::default();
            let source = from_iter(input.clone().into_iter(), *channels, *from);
            let output = Resample::new(source, *from, config).collect::<Vec<_>>();
            input == output
        }

        /// Check that resampling does not change the audio duration, except by a negligible
        /// amount (± 1ms). Reproduces #316.
        fn preserve_durations(d: Duration, freq: f32, to: TestSampleRate) -> TestResult {
            use crate::source::{SineWave, Source};
            if !freq.is_normal() || freq <= 0.0 || d > Duration::from_secs(1) {
                return TestResult::discard();
            }

            let source = SineWave::new(freq).take_duration(d);
            let from = source.sample_rate();

            let config = ResampleConfig::poly().degree(Poly::Linear).build();
            let resampled = Resample::new(source, *to, config);
            let duration = Duration::from_secs_f32(resampled.count() as f32 / to.get() as f32);

            let delta = duration.abs_diff(d);
            TestResult::from_bool(delta < Duration::from_millis(1))
        }
    }

    /// Helper to create interleaved multi-channel test data using SineWave sources.
    fn create_test_input(frames: usize, channels: u16) -> Vec<Sample> {
        let frequencies = [440.0, 1000.0];
        let total_samples = frames * channels as usize;
        let mut input = Vec::with_capacity(total_samples);

        // Create a SineWave for each channel
        let mut waves: Vec<_> = (0..channels)
            .map(|ch| SineWave::new(frequencies[ch as usize % frequencies.len()]))
            .collect();

        // Interleave samples from each channel
        for _ in 0..frames {
            for wave in waves.iter_mut() {
                input.push(wave.next().unwrap());
            }
        }
        input
    }

    /// Test various ratio types: integer, fractional, and reciprocal.
    #[test]
    fn test_sample_rate_conversions() {
        let test_cases = [
            // (from_rate, to_rate, channels, description)
            (1000, 7000, 1, "integer upsample 7x"),
            (2000, 3000, 2, "fractional upsample 1.5x"),
            (12000, 2400, 1, "integer downsample 1/5x"),
            (48000, 44100, 2, "fractional downsample (DVD to CD)"),
            (8000, 48001, 1, "async sinc"),
        ];

        let configs: &[(&str, ResampleConfig)] = &[
            ("poly", ResampleConfig::poly().build()),
            ("sinc", ResampleConfig::sinc().build()),
        ];

        for (config_name, config) in configs {
            for (from_rate, to_rate, channels, desc) in test_cases {
                let from = SampleRate::new(from_rate).unwrap();
                let to = SampleRate::new(to_rate).unwrap();
                let ch = ChannelCount::new(channels).unwrap();

                let input_frames = 100;
                let input = create_test_input(input_frames, channels);
                let input_samples = input.len();

                let source = from_iter(input.into_iter(), ch, from);
                let resampler = Resample::new(source, to, config.clone());

                let size_hint_lower = resampler.size_hint().0;
                let output_count = resampler.count();

                assert_eq!(
                    output_count, size_hint_lower,
                    "[{config_name}] {desc}: size_hint {size_hint_lower} should equal actual output {output_count}",
                );

                let ratio = to.get() as f64 / from.get() as f64;
                let expected_samples = (input_samples as f64 * ratio).ceil() as usize;

                assert_eq!(
                    output_count.abs_diff(expected_samples), 0,
                    "[{config_name}] {desc}: expected {expected_samples} samples, got {output_count}",
                );
            }
        }
    }
}
