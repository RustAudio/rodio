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
//! For advanced control, use [`SampleRateConverter`] directly:
//!
//! ```rust
//! use rodio::math::nz;
//! use rodio::source::{SineWave, Source, ResampleConfig};
//! use rodio::conversions::{SampleRateConverter, Interpolation, WindowFunction};
//!
//! let source = SineWave::new(440.0);
//! let config = ResampleConfig::sinc()                  // Sinc resampling
//!     .sinc_len(nz!(256))                              // 256-tap filter
//!     .interpolation(Interpolation::Cubic)             // Cubic interpolation
//!     .window(WindowFunction::BlackmanHarris2)         // Squared Blackman-Harris window
//!     .chunk_size(nz!(512))                            // Low latency (5.3 ms @ 1-channel 96 kHz)
//!     .build();
//! let resampled = SampleRateConverter::new(source, nz!(96000), config);
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

use crate::source::{reset_seek_span_tracking, SeekError};
use crate::{
    common::{ChannelCount, Sample, SampleRate},
    math::gcd,
    Source,
};

mod buffer;
mod builder;
mod rubato;
mod types;
pub(crate) use types::{InFrameCount, InSamples, OutFrameCount, OutSamples};
#[cfg(test)]
mod tests;

#[cfg(feature = "rubato-fft")]
use rubato::RubatoFftResample;
use rubato::{ResampleInner, RubatoAsyncResample};

pub use builder::{
    Interpolation, Poly, PolyConfigBuilder, ResampleConfig, SincConfigBuilder, WindowFunction,
};

/// Maximum for optimized fixed-ratio resampling: 44.1 and 384 kHz (147:1280).
const MAX_FIXED_RATIO: u32 = 1280;

/// Resamples an audio source to a target sample rate using Rubato.
#[derive(Debug)]
pub struct SampleRateConverter<I>
where
    I: Source,
{
    // Kept in Option so we can take ownership for in-place recreation on parameter change
    inner: Option<ResampleInner<I>>,
    target_rate: SampleRate,
    config: ResampleConfig,
    cached_input_span_len: Option<usize>,
    // True when a format change was detected at a span boundary but the output buffer still
    // has samples from the old format. Recreation is deferred until the buffer is drained so
    // fill_input_buffer never reads the next span's samples with the wrong channel count.
    pending_recreate: bool,
}

impl<I> Clone for SampleRateConverter<I>
where
    I: Source + Clone,
{
    fn clone(&self) -> Self {
        // Shallow clone: this resets filter state
        let source = self.inner().clone();
        SampleRateConverter::new(source, self.target_rate, self.config.clone())
    }
}

impl<I> SampleRateConverter<I>
where
    I: Source,
{
    /// Create a new resampler with the given configuration.
    pub fn new(source: I, target_rate: SampleRate, config: ResampleConfig) -> Self {
        let inner = Self::create_resampler(source, target_rate, &config);

        #[cfg(debug_assertions)]
        if matches!(inner, ResampleInner::Sinc(_)) {
            let msg = "Warning: async sinc resampling is active. This is CPU-intensive and may \
                 produce choppy audio in a debug build. Either use an integer-multiple ratio \
                 or compile with --release.";
            #[cfg(feature = "tracing")]
            tracing::warn!(msg);
            #[cfg(not(feature = "tracing"))]
            eprintln!("{}", msg);
        }

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
            pending_recreate: false,
        }
    }

    /// Rebuild for the format the input has moved on to.
    fn recreate(&mut self) {
        let source = self.inner.take().expect("always set").into_inner();
        self.inner = Some(Self::create_resampler(
            source,
            self.target_rate,
            &self.config,
        ));
        self.pending_recreate = false;
        self.cached_input_span_len = self.resampler().input().current_span_len();
    }

    fn next_resampled(&mut self) -> Option<Sample> {
        match self.resampler_mut() {
            ResampleInner::Passthrough { source, .. } => source.next(),
            ResampleInner::Poly(resampler) => resampler.next_sample(),
            ResampleInner::Sinc(resampler) => resampler.next_sample(),
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => resampler.next_sample(),
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
                input_span_pos: InSamples::ZERO,
                channels,
                source_rate,
            }
        } else {
            match config {
                ResampleConfig::Poly { degree, chunk_size } => {
                    let resampler =
                        RubatoAsyncResample::new_poly(source, target_rate, *chunk_size, *degree)
                            .expect("Failed to create polynomial resampler");
                    ResampleInner::Poly(resampler)
                }
                ResampleConfig::Sinc(sinc) => {
                    let mut sinc = sinc.clone();
                    #[cfg(feature = "rubato-fft")]
                    if sinc.is_supported_fixed_ratio(target_rate, source_rate) {
                        let resampler = RubatoFftResample::new(
                            source,
                            target_rate,
                            sinc.chunk_size,
                            sinc.sub_chunks,
                        )
                        .expect("Failed to create FFT resampler");
                        return ResampleInner::Fft(resampler);
                    }

                    if sinc.is_supported_fixed_ratio(target_rate, source_rate) {
                        sinc.interpolation = Interpolation::Nearest;
                        let g = gcd(target_rate.get(), source_rate.get());
                        let numer = target_rate.get() / g;
                        let denom = source_rate.get() / g;
                        let ratio = numer.max(denom) as usize;
                        sinc.oversampling_factor = ratio;
                    }
                    ResampleInner::Sinc(sinc.build(source, target_rate))
                }
            }
        }
    }

    #[inline]
    fn resampler(&self) -> &ResampleInner<I> {
        self.inner.as_ref().unwrap()
    }

    #[inline]
    fn resampler_mut(&mut self) -> &mut ResampleInner<I> {
        self.inner.as_mut().unwrap()
    }

    /// Returns a reference to the inner source.
    #[inline]
    pub fn inner(&self) -> &I {
        self.resampler().input()
    }

    /// Returns a mutable reference to the inner source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        match self.resampler_mut() {
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

    /// Returns `(at_boundary, parameters_changed)` given span tracking state.
    ///
    /// Two modes:
    /// - Counting (`cached_span_len` is `Some`): boundary when `samples_consumed >= span_len`
    /// - Detection (`cached_span_len` is `None`): boundary when parameters change (post-seek)
    fn detect_boundary(
        cached_span_len: Option<usize>,
        samples_consumed: InSamples,
        current_channels: ChannelCount,
        expected_channels: ChannelCount,
        current_rate: SampleRate,
        expected_rate: SampleRate,
    ) -> (bool, bool) {
        let known_boundary = cached_span_len.map(|len| samples_consumed.raw() >= len);
        // In counting mode: only check parameters at boundary
        // In detection mode: check parameters at every sample until a boundary is detected
        let parameters_changed = if known_boundary.is_none_or(|at| at) {
            current_channels != expected_channels || current_rate != expected_rate
        } else {
            false
        };
        (
            known_boundary.unwrap_or(parameters_changed),
            parameters_changed,
        )
    }
}

impl<I> Source for SampleRateConverter<I>
where
    I: Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        match self.resampler() {
            ResampleInner::Passthrough { source, .. } => source.current_span_len(),
            ResampleInner::Poly(resampler) | ResampleInner::Sinc(resampler) => {
                resampler.span_length()
            }
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => resampler.span_length(),
        }
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.target_rate
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.resampler().input().channels()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.resampler().input().total_duration()
    }

    #[inline]
    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        match self.resampler_mut() {
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

        self.pending_recreate = false;
        let input_span_len = self.resampler().input().current_span_len();

        // Only the passthrough counts samples against the span length;
        // `reset` already cleared the resamplers' own tracking.
        if let ResampleInner::Passthrough { input_span_pos, .. } =
            self.inner.as_mut().expect("always set")
        {
            reset_seek_span_tracking(
                input_span_pos.raw_mut(),
                &mut self.cached_input_span_len,
                position,
                input_span_len,
            );
        }

        Ok(())
    }
}

impl<I> Iterator for SampleRateConverter<I>
where
    I: Source,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // If a format change was detected at the previous span boundary, wait until the
        // output buffer is fully drained before recreating the resampler. This guarantees
        // that fill_input_buffer only ever reads from the current span.
        let sample = loop {
            if self.pending_recreate {
                let output_empty = match self.resampler() {
                    ResampleInner::Passthrough { .. } => true,
                    ResampleInner::Poly(r) | ResampleInner::Sinc(r) => !r.output_has_samples(),
                    #[cfg(feature = "rubato-fft")]
                    ResampleInner::Fft(r) => !r.output_has_samples(),
                };
                if output_empty {
                    self.recreate();
                }
            }

            match self.next_resampled() {
                Some(sample) => break sample,
                // Built for one format, a resampler stops where its input
                // changes rate or channels. Rebuilding matches the format
                // it stopped at, so this retries at most once.
                None if self.resampler().built_for_input() => return None,
                None => self.recreate(),
            }
        };

        // Only the passthrough tracks spans here; a resampler stops at a
        // format change itself while filling, and the loop above rebuilds it.
        let ResampleInner::Passthrough {
            input_span_pos,
            channels,
            source_rate,
            source,
        } = self.resampler_mut()
        else {
            return Some(sample);
        };
        let input_span_len = source.current_span_len();
        // No span length means the format is stable by contract.
        if input_span_len.is_none() {
            return Some(sample);
        }
        *input_span_pos += 1usize;
        let (expected_channels, expected_rate, samples_consumed) =
            (*channels, *source_rate, *input_span_pos);

        let input = self.resampler().input();
        let (at_boundary, parameters_changed) = Self::detect_boundary(
            self.cached_input_span_len,
            samples_consumed,
            input.channels(),
            expected_channels,
            input.sample_rate(),
            expected_rate,
        );

        if at_boundary {
            self.cached_input_span_len = input_span_len;
            if parameters_changed {
                self.pending_recreate = true;
            } else if let ResampleInner::Passthrough { input_span_pos, .. } = self.resampler_mut() {
                *input_span_pos = InSamples::ZERO;
            }
        }

        Some(sample)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.resampler() {
            ResampleInner::Passthrough { source, .. } => source.size_hint(),
            ResampleInner::Poly(resampler) | ResampleInner::Sinc(resampler) => {
                let adjusted_for_resampling = |samples| {
                    InSamples(samples).resampled_by(resampler.resample_ratio)
                        + resampler.output.len()
                        + resampler
                            .frames_being_resampled
                            .samples(resampler.output.channels)
                };
                let (lower, upper) = resampler.input.size_hint();
                let lower = adjusted_for_resampling(lower);
                let upper = upper.map(adjusted_for_resampling);
                (lower.raw(), upper.as_ref().map(OutSamples::raw))
            }
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => {
                let adjusted_for_resampling = |samples| {
                    InSamples(samples).resampled_by(resampler.resample_ratio)
                        + resampler.output.len()
                        + resampler
                            .frames_being_resampled
                            .samples(resampler.output.channels)
                };
                let (lower, upper) = resampler.input.size_hint();
                let lower = adjusted_for_resampling(lower);
                let upper = upper.map(adjusted_for_resampling);
                (lower.raw(), upper.as_ref().map(OutSamples::raw))
            }
        }
    }
}
