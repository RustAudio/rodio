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

use std::{num::NonZero, time::Duration};

use dasp_sample::Sample as _;
use num_rational::Ratio;
use rubato::{Indexing, Resampler as RubatoResampler};

use super::{reset_seek_span_tracking, SeekError};
use crate::{
    common::{ChannelCount, Sample, SampleRate},
    Float, Source,
};

const DEFAULT_CHUNK_SIZE: usize = 1024;
#[cfg(feature = "rubato-fft")]
const DEFAULT_SUB_CHUNKS: usize = 1;

/// Maximum for optimized fixed-ratio resampling: 44.1 and 384 kHz (147:1280).
const MAX_FIXED_RATIO: u32 = 1280;

/// Polynomial interpolation degree, no anti-aliasing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Poly {
    /// Zero-order hold - nearest neighbor sampling.
    ///
    /// Simply picks the nearest input sample without interpolation.
    /// Creates a "stepped" waveform.
    Nearest,

    /// Linear interpolation between 2 samples.
    #[default]
    Linear,

    /// Cubic interpolation using 4 samples.
    Cubic,

    /// Quintic interpolation using 6 samples.
    Quintic,

    /// Septic interpolation using 8 samples.
    Septic,
}

/// Sinc interpolation type.
///
/// Controls how intermediate values are calculated between precomputed sinc points
/// in the windowed sinc filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sinc {
    /// No interpolation - picks nearest intermediate point.
    ///
    /// Optimal when upsampling by exact ratios (e.g., 48kHz and 96kHz) and the oversampling factor
    /// is equal to the ratio. In these cases, no unnecessary computations are performed and the
    /// result is equivalent to that of synchronous resampling.
    Nearest,

    /// Linear interpolation between two nearest points.
    ///
    /// Relatively fast, but needs a large number of intermediate points to push the resampling
    /// artefacts below the noise floor.
    #[default]
    Linear,

    /// Quadratic interpolation using three nearest points.
    ///
    /// The computation time lies approximately halfway between that of linear and quadratic
    /// interpolation.
    Quadratic,

    /// Cubic interpolation using four nearest points.
    ///
    /// The computation time is approximately twice as long as that of linear interpolation, but it
    /// requires much fewer intermediate points for a good result.
    Cubic,
}

impl From<Sinc> for rubato::SincInterpolationType {
    fn from(sinc: Sinc) -> Self {
        match sinc {
            Sinc::Nearest => rubato::SincInterpolationType::Nearest,
            Sinc::Linear => rubato::SincInterpolationType::Linear,
            Sinc::Quadratic => rubato::SincInterpolationType::Quadratic,
            Sinc::Cubic => rubato::SincInterpolationType::Cubic,
        }
    }
}

impl From<Poly> for rubato::PolynomialDegree {
    fn from(poly: Poly) -> Self {
        match poly {
            Poly::Nearest => rubato::PolynomialDegree::Nearest,
            Poly::Linear => rubato::PolynomialDegree::Linear,
            Poly::Cubic => rubato::PolynomialDegree::Cubic,
            Poly::Quintic => rubato::PolynomialDegree::Quintic,
            Poly::Septic => rubato::PolynomialDegree::Septic,
        }
    }
}

/// Window functions for sinc filter.
///
/// The window function is applied to the sinc filter to reduce ripple artifacts and control the
/// trade-off between transition bandwidth and stopband attenuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowFunction {
    /// Hann window: ~44 dB stopband attenuation, fast -18 dB/octave rolloff.
    ///
    /// Good transition band but moderate rejection. Suitable for less critical applications.
    Hann,

    /// Squared Hann: ~50 dB stopband attenuation, medium -12 dB/octave rolloff.
    ///
    /// Better rejection than Hann with slightly wider transition band.
    Hann2,

    /// Blackman window: ~75 dB stopband attenuation, fast -18 dB/octave rolloff.
    ///
    /// Excellent rejection with sharp cutoff.
    Blackman,

    /// Squared Blackman: ~81 dB stopband attenuation, medium -12 dB/octave rolloff.
    ///
    /// Very good rejection with moderate transition band.
    Blackman2,

    /// Blackman-Harris window: ~92 dB stopband attenuation, slow -6 dB/octave rolloff.
    ///
    /// Extremely high rejection but wider transition band.
    BlackmanHarris,

    /// Squared Blackman-Harris: ~98 dB stopband attenuation, very slow -3 dB/octave rolloff.
    ///
    /// Maximum stopband rejection, widest transition band.
    #[default]
    BlackmanHarris2,
}

impl From<WindowFunction> for rubato::WindowFunction {
    fn from(window: WindowFunction) -> Self {
        match window {
            WindowFunction::Hann => rubato::WindowFunction::Hann,
            WindowFunction::Hann2 => rubato::WindowFunction::Hann2,
            WindowFunction::Blackman => rubato::WindowFunction::Blackman,
            WindowFunction::Blackman2 => rubato::WindowFunction::Blackman2,
            WindowFunction::BlackmanHarris => rubato::WindowFunction::BlackmanHarris,
            WindowFunction::BlackmanHarris2 => rubato::WindowFunction::BlackmanHarris2,
        }
    }
}

/// Builder for polynomial resampling configuration without anti-aliasing.
#[derive(Debug, Clone)]
pub struct PolyConfigBuilder {
    degree: Poly,
    chunk_size: usize,
}

impl Default for PolyConfigBuilder {
    fn default() -> Self {
        Self {
            degree: Poly::default(),
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }
}

/// Builder for sinc resampling configuration with anti-aliasing.
#[derive(Debug, Clone)]
pub struct SincConfigBuilder {
    sinc_len: usize,
    oversampling_factor: usize,
    interpolation: Sinc,
    window: WindowFunction,
    f_cutoff: Float,
    chunk_size: usize,
    #[cfg(feature = "rubato-fft")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rubato-fft")))]
    sub_chunks: usize,
}

impl Default for SincConfigBuilder {
    fn default() -> Self {
        Self {
            sinc_len: 256,
            window: WindowFunction::default(),
            oversampling_factor: 128,
            interpolation: Sinc::default(),
            f_cutoff: 0.95,
            chunk_size: DEFAULT_CHUNK_SIZE,
            #[cfg(feature = "rubato-fft")]
            sub_chunks: DEFAULT_SUB_CHUNKS,
        }
    }
}

/// Resampling configuration.
///
/// Specifies the algorithm and parameters for sample rate conversion.
///
/// # Examples
///
/// ```rust
/// use rodio::math::nz;
/// use rodio::source::{resample::Poly, ResampleConfig};
///
/// // Use presets
/// let config = ResampleConfig::balanced();
/// let config = ResampleConfig::fast();
/// let config = ResampleConfig::accurate();
///
/// // Customize from builder
/// let config = ResampleConfig::sinc().chunk_size(nz!(512));
/// let config = ResampleConfig::poly().degree(Poly::Cubic);
/// ```
#[derive(Debug, Clone)]
pub enum ResampleConfig {
    /// Polynomial resampling (fast, no anti-aliasing)
    Poly {
        /// Polynomial degree
        degree: Poly,
        /// Desired chunk size in frames
        chunk_size: usize,
    },
    /// Sinc resampling (high quality, anti-aliasing)
    Sinc {
        /// Length of the windowed sinc interpolation filter
        sinc_len: usize,
        /// The number of intermediate points to use for interpolation
        oversampling_factor: usize,
        /// Interpolation type for filter table lookup
        interpolation: Sinc,
        /// Window function to use
        window: WindowFunction,
        /// Cutoff frequency of the sinc interpolation filter relative to Nyquist (0.0-1.0)
        f_cutoff: Float,
        /// Desired chunk size in frames
        chunk_size: usize,
        /// Desired number of sub chunks to use for processing
        #[cfg(feature = "rubato-fft")]
        #[cfg_attr(docsrs, doc(cfg(feature = "rubato-fft")))]
        sub_chunks: usize,
    },
}

// Implementation for ResampleConfig with presets and entry points
impl ResampleConfig {
    /// Create a very fast sinc resampling configuration.
    pub fn very_fast() -> Self {
        let sinc_len = 64;
        let window = WindowFunction::Hann2;
        Self::Sinc {
            sinc_len,
            window,
            oversampling_factor: 1024,
            interpolation: Sinc::Linear,
            f_cutoff: rubato::calculate_cutoff(sinc_len, window.into()),
            chunk_size: DEFAULT_CHUNK_SIZE,
            #[cfg(feature = "rubato-fft")]
            sub_chunks: DEFAULT_SUB_CHUNKS,
        }
    }

    /// Create a fast sinc resampling configuration.
    pub fn fast() -> Self {
        let sinc_len = 128;
        let window = WindowFunction::Blackman2;
        Self::Sinc {
            sinc_len,
            window,
            oversampling_factor: 1024,
            interpolation: Sinc::Linear,
            f_cutoff: rubato::calculate_cutoff(sinc_len, window.into()),
            chunk_size: DEFAULT_CHUNK_SIZE,
            #[cfg(feature = "rubato-fft")]
            sub_chunks: DEFAULT_SUB_CHUNKS,
        }
    }

    /// Create a balanced sinc resampling configuration.
    pub fn balanced() -> Self {
        let sinc_len = 192;
        let window = WindowFunction::BlackmanHarris2;
        Self::Sinc {
            sinc_len,
            window,
            oversampling_factor: 512,
            interpolation: Sinc::Quadratic,
            f_cutoff: rubato::calculate_cutoff(sinc_len, window.into()),
            chunk_size: DEFAULT_CHUNK_SIZE,
            #[cfg(feature = "rubato-fft")]
            sub_chunks: DEFAULT_SUB_CHUNKS,
        }
    }

    /// Create an accurate sinc resampling configuration.
    pub fn accurate() -> Self {
        let sinc_len = 256;
        let window = WindowFunction::BlackmanHarris2;
        Self::Sinc {
            sinc_len,
            window,
            oversampling_factor: 256,
            interpolation: Sinc::Cubic,
            f_cutoff: rubato::calculate_cutoff(sinc_len, window.into()),
            chunk_size: DEFAULT_CHUNK_SIZE,
            #[cfg(feature = "rubato-fft")]
            sub_chunks: DEFAULT_SUB_CHUNKS,
        }
    }

    /// Create a polynomial resampling configuration builder.
    pub fn poly() -> PolyConfigBuilder {
        PolyConfigBuilder::default()
    }

    /// Create a sinc resampling configuration builder.
    pub fn sinc() -> SincConfigBuilder {
        SincConfigBuilder::default()
    }
}

impl Default for ResampleConfig {
    fn default() -> Self {
        Self::balanced()
    }
}

impl PolyConfigBuilder {
    /// Set the polynomial degree for interpolation.
    pub fn degree(mut self, degree: Poly) -> Self {
        self.degree = degree;
        self
    }

    /// Set number of audio frames processed at once (typical range: 32-2048).
    ///
    /// Smaller chunks reduce latency (time delay through the resampler) but increase per-sample
    /// overhead. One frame contains one sample per channel. Default is 1024 frames, which at 48
    /// kHz is ~10.7ms latency.
    pub fn chunk_size(mut self, size: NonZero<usize>) -> Self {
        self.chunk_size = size.get();
        self
    }

    /// Build the final [`ResampleConfig`].
    pub fn build(self) -> ResampleConfig {
        ResampleConfig::Poly {
            degree: self.degree,
            chunk_size: self.chunk_size,
        }
    }
}

impl From<PolyConfigBuilder> for ResampleConfig {
    fn from(builder: PolyConfigBuilder) -> Self {
        builder.build()
    }
}

impl SincConfigBuilder {
    /// Set the length of the sinc filter in taps (typical range: 32-2048).
    ///
    /// Longer filters provide better quality but use more CPU.
    pub fn sinc_len(mut self, len: NonZero<usize>) -> Self {
        self.sinc_len = len.get();
        self
    }

    /// Set oversampling factor (typical range: 64-4096).
    ///
    /// Higher values improve interpolation accuracy but increase memory usage.
    pub fn oversampling_factor(mut self, factor: NonZero<usize>) -> Self {
        self.oversampling_factor = factor.get();
        self
    }

    /// Set interpolation type.
    pub fn interpolation(mut self, interpolator: Sinc) -> Self {
        self.interpolation = interpolator;
        self
    }

    /// Set window function.
    pub fn window(mut self, window: WindowFunction) -> Self {
        self.window = window;
        self
    }

    /// Set the cutoff frequency as fraction of the Nyquist frequency.
    ///
    /// Value should be between 0.0 and 1.0, where 1.0 represents the Nyquist frequency (half the
    /// sample rate) of the input sampling rate or output sampling rate, whichever is lower. The
    /// cutoff determines where the anti-aliasing filter begins to attenuate frequencies.
    ///
    /// Lower values provide more anti-aliasing protection but reduce high frequency response.
    ///
    /// # Panics
    ///
    /// Panics if cutoff is not in range 0.0-1.0.
    pub fn f_cutoff(mut self, cutoff: Float) -> Self {
        assert!(
            (0.0..=1.0).contains(&cutoff),
            "f_cutoff must be between 0.0 and 1.0"
        );
        self.f_cutoff = cutoff;
        self
    }

    /// Set the length of the sinc filter, the window function, automatically calculating
    /// the cutoff frequency for the combination of the two.
    pub fn with_sinc_and_window(
        mut self,
        sinc_len: NonZero<usize>,
        window: WindowFunction,
    ) -> Self {
        self.sinc_len = sinc_len.get();
        self.window = window;
        self.f_cutoff = rubato::calculate_cutoff(sinc_len.get(), window.into());
        self
    }

    /// Set chunk size for processing (typical range: 512-4096).
    ///
    /// This balances between efficiency and memory usage. If the device sink uses a fixed buffer
    /// size, then this number of frames is a good choice for the resampler chunk size.
    pub fn chunk_size(mut self, size: NonZero<usize>) -> Self {
        self.chunk_size = size.get();
        self
    }

    /// Set number of sub-chunks for FFT resampling.
    ///
    /// The delay of the resampler can be reduced by increasing the number of sub-chunks. A large
    /// number of sub-chunks reduces the cutoff frequency of the anti-aliasing filter. It is
    /// recommended to set keep this at 1 unless this leads to an unacceptably large delay.
    #[cfg(feature = "rubato-fft")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rubato-fft")))]
    pub fn sub_chunks(mut self, count: NonZero<usize>) -> Self {
        self.sub_chunks = count.get();
        self
    }

    /// Build the final [`ResampleConfig`].
    pub fn build(self) -> ResampleConfig {
        ResampleConfig::Sinc {
            sinc_len: self.sinc_len,
            oversampling_factor: self.oversampling_factor,
            interpolation: self.interpolation,
            window: self.window,
            f_cutoff: self.f_cutoff,
            chunk_size: self.chunk_size,
            #[cfg(feature = "rubato-fft")]
            #[cfg_attr(docsrs, doc(cfg(feature = "rubato-fft")))]
            sub_chunks: self.sub_chunks,
        }
    }
}

impl From<SincConfigBuilder> for ResampleConfig {
    fn from(builder: SincConfigBuilder) -> Self {
        builder.build()
    }
}

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

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum ResampleInner<I: Source> {
    /// Passthrough when source rate is equal to the target rate
    Passthrough {
        source: I,
        input_span_pos: usize,
        channels: ChannelCount,
        source_rate: SampleRate,
    },

    /// Polynomial resampling (fast, no anti-aliasing)
    Poly(RubatoAsyncResample<I>),

    /// Sinc resampling (with anti-aliasing)
    Sinc(RubatoAsyncResample<I>),

    /// FFT resampling for fixed ratios (synchronous resampling)
    #[cfg(feature = "rubato-fft")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rubato-fft")))]
    Fft(RubatoFftResample<I>),
}

impl<I: Source> ResampleInner<I> {
    /// Get a reference to the inner input source
    #[inline]
    fn input(&self) -> &I {
        match self {
            ResampleInner::Passthrough { source, .. } => source,
            ResampleInner::Poly(resampler) => &resampler.input,
            ResampleInner::Sinc(resampler) => &resampler.input,
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => &resampler.input,
        }
    }

    /// Extract the inner input source, consuming the resampler
    #[inline]
    fn into_inner(self) -> I {
        match self {
            ResampleInner::Passthrough { source, .. } => source,
            ResampleInner::Poly(resampler) => resampler.input,
            ResampleInner::Sinc(resampler) => resampler.input,
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => resampler.input,
        }
    }
}

/// Generic wrapper around Rubato resamplers for sample-by-sample iteration.
#[derive(Debug)]
struct RubatoResample<I: Source, R: rubato::Resampler<Sample>> {
    input: I,
    resampler: R,

    input_buffer: Box<[Sample]>,
    input_frame_count: usize,

    output_buffer: Box<[Sample]>,
    output_buffer_pos: usize,
    output_buffer_len: usize,

    channels: ChannelCount,
    source_rate: SampleRate,

    input_samples_consumed: usize,
    input_exhausted: bool,

    total_input_frames: usize,
    total_output_samples: usize,
    expected_output_samples: usize,

    /// The number of real (non-flush) frames currently in the input buffer.
    real_frames_in_buffer: usize,

    output_delay_remaining: usize,
    resample_ratio: Float,
    indexing: Indexing,
}

/// Type alias for Async (polynomial/sinc) resampler.
type RubatoAsyncResample<I> = RubatoResample<I, rubato::Async<Sample>>;

impl<I: Source, R: rubato::Resampler<Sample>> RubatoResample<I, R> {
    /// Calculate the number of output samples to skip for delay compensation.
    fn calculate_delay_compensation(resampler: &R, channels: ChannelCount) -> usize {
        // Skip delay-1 frames to align the first output frame with input position 0.
        let delay_frames = resampler.output_delay();
        let delay_to_skip = delay_frames.saturating_sub(1);
        delay_to_skip * channels.get() as usize
    }

    fn reset(&mut self) {
        self.resampler.reset();
        self.output_buffer_pos = 0;
        self.output_buffer_len = 0;
        self.input_frame_count = 0;
        self.input_samples_consumed = 0;
        self.input_exhausted = false;
        self.total_input_frames = 0;
        self.total_output_samples = 0;
        self.expected_output_samples = 0;
        self.real_frames_in_buffer = 0;
        self.indexing.partial_len = None;
        self.output_delay_remaining =
            Self::calculate_delay_compensation(&self.resampler, self.channels);
    }

    fn next_sample(&mut self) -> Option<Sample> {
        let num_channels = self.channels.get() as usize;
        loop {
            // If we have buffered output, return it
            if self.output_buffer_pos < self.output_buffer_len {
                let sample = self.output_buffer[self.output_buffer_pos];
                self.output_buffer_pos += 1;
                self.total_output_samples += 1;

                if self.total_output_samples > self.expected_output_samples {
                    // Cut off filter artifacts after input is exhausted
                    return None;
                }

                return Some(sample);
            }

            // Need more input - first check if we're completely done
            if self.input_exhausted
                && self.input_frame_count == 0
                && self.total_output_samples >= self.expected_output_samples
            {
                return None;
            }

            // Fill input buffer - accumulate frames until we hit needed amount or run out of input
            let needed_input = self.resampler.input_frames_next();
            let frames_before = self.input_frame_count;
            while self.input_frame_count < needed_input && !self.input_exhausted {
                let sample_pos = self.input_frame_count * num_channels;
                for ch in 0..num_channels {
                    if let Some(sample) = self.input.next() {
                        self.input_buffer[sample_pos + ch] = sample;
                    } else {
                        self.input_exhausted = true;
                        break;
                    }
                }
                if !self.input_exhausted {
                    self.input_frame_count += 1;
                    self.real_frames_in_buffer += 1;
                }
            }

            // If we have no input, flush the filter tail with zeros
            if self.input_frame_count == 0 {
                // Zero-pad a full chunk to drain the filter delay
                self.input_buffer[..needed_input * num_channels].fill(Sample::EQUILIBRIUM);
                self.input_frame_count = needed_input;
                // real_frames_in_buffer stays at 0 - these are flush frames
            }

            // We can process with fewer frames than needed using partial_len when the input is
            // exhausted. If we don't have enough input and more is coming, wait.
            let made_progress = self.input_frame_count > frames_before;
            if self.input_frame_count < needed_input && !self.input_exhausted && made_progress {
                continue;
            }

            let actual_frames = self.input_frame_count;

            // Prevent stack allocations in the hot path by reusing the indexing struct
            let indexing_ref = if actual_frames < needed_input {
                self.indexing.partial_len = Some(actual_frames);
                Some(&self.indexing)
            } else {
                None
            };

            let (frames_in, frames_out) = {
                // InterleavedSlice is a zero-cost abstraction - no heap allocation occurs here
                let input_adapter = audioadapter_buffers::direct::InterleavedSlice::new(
                    &self.input_buffer,
                    num_channels,
                    actual_frames,
                )
                .ok()?;

                let num_frames = self.output_buffer.len() / num_channels;
                let mut output_adapter = audioadapter_buffers::direct::InterleavedSlice::new_mut(
                    &mut self.output_buffer,
                    num_channels,
                    num_frames,
                )
                .ok()?;

                self.resampler
                    .process_into_buffer(&input_adapter, &mut output_adapter, indexing_ref)
                    .ok()?
            };

            // If no output was produced and input is exhausted, we're done
            if frames_out == 0 && self.input_exhausted {
                return None;
            }

            // When using partial_len, Rubato may report consuming more frames than we
            // actually provided (it counts the zero-padded frames). Clamp to actual.
            let actual_consumed = frames_in.min(actual_frames);
            self.input_samples_consumed += actual_consumed * num_channels;

            // Only count real (non-flush) frames toward expected output
            let real_consumed = actual_consumed.min(self.real_frames_in_buffer);
            self.real_frames_in_buffer -= real_consumed;
            self.total_input_frames += real_consumed;
            self.expected_output_samples = (self.total_input_frames as Float * self.resample_ratio)
                .ceil() as usize
                * num_channels;

            // Shift remaining input samples to beginning of buffer
            if actual_consumed < self.input_frame_count {
                let src_start = actual_consumed * num_channels;
                let src_end = self.input_frame_count * num_channels;
                self.input_buffer.copy_within(src_start..src_end, 0);
            }
            self.input_frame_count -= actual_consumed;

            self.output_buffer_pos = 0;
            self.output_buffer_len = frames_out * num_channels;

            // Skip warmup delay samples
            if self.output_delay_remaining > 0 {
                let samples_to_skip = self.output_delay_remaining.min(self.output_buffer_len);
                self.output_buffer_pos += samples_to_skip;
                self.output_delay_remaining -= samples_to_skip;
            }
        }
    }
}

impl<I: Source> RubatoAsyncResample<I> {
    fn new_poly(
        input: I,
        target_rate: SampleRate,
        chunk_size: usize,
        degree: Poly,
    ) -> Result<Self, String> {
        let source_rate = input.sample_rate();
        let channels = input.channels();

        let resample_ratio = target_rate.get() as Float / source_rate.get() as Float;

        let resampler = rubato::Async::new_poly(
            resample_ratio as _,
            1.0,
            degree.into(),
            chunk_size,
            channels.get() as usize,
            rubato::FixedAsync::Output,
        )
        .map_err(|e| format!("Failed to create polynomial resampler: {:?}", e))?;

        let input_buf_size = resampler.input_frames_max();
        let output_buf_size = resampler.output_frames_max();

        let output_delay_remaining =
            RubatoResample::<I, rubato::Async<Sample>>::calculate_delay_compensation(
                &resampler, channels,
            );

        Ok(Self {
            input,
            resampler,
            input_buffer: vec![Sample::EQUILIBRIUM; input_buf_size * channels.get() as usize]
                .into_boxed_slice(),
            input_frame_count: 0,
            output_buffer: vec![Sample::EQUILIBRIUM; output_buf_size * channels.get() as usize]
                .into_boxed_slice(),
            output_buffer_pos: 0,
            output_buffer_len: 0,
            channels,
            source_rate,
            input_samples_consumed: 0,
            input_exhausted: false,
            output_delay_remaining,
            total_input_frames: 0,
            total_output_samples: 0,
            expected_output_samples: 0,
            real_frames_in_buffer: 0,
            resample_ratio,
            indexing: Indexing {
                input_offset: 0,
                output_offset: 0,
                partial_len: None,
                active_channels_mask: None,
            },
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn new_sinc(
        input: I,
        target_rate: SampleRate,
        chunk_size: usize,
        sinc_len: usize,
        f_cutoff: Float,
        oversampling_factor: usize,
        interpolation: Sinc,
        window: WindowFunction,
    ) -> Result<Self, String> {
        let source_rate = input.sample_rate();
        let channels = input.channels();

        let parameters = rubato::SincInterpolationParameters {
            sinc_len,
            f_cutoff: f_cutoff as _,
            oversampling_factor,
            interpolation: interpolation.into(),
            window: window.into(),
        };

        let resample_ratio = target_rate.get() as Float / source_rate.get() as Float;

        let resampler = rubato::Async::new_sinc(
            resample_ratio as _,
            1.0,
            &parameters,
            chunk_size,
            channels.get() as usize,
            rubato::FixedAsync::Output,
        )
        .map_err(|e| format!("Failed to create sinc resampler: {:?}", e))?;

        let input_buf_size = resampler.input_frames_max();
        let output_buf_size = resampler.output_frames_max();

        let output_delay_remaining =
            RubatoResample::<I, rubato::Async<Sample>>::calculate_delay_compensation(
                &resampler, channels,
            );

        Ok(Self {
            input,
            resampler,
            input_buffer: vec![Sample::EQUILIBRIUM; input_buf_size * channels.get() as usize]
                .into_boxed_slice(),
            input_frame_count: 0,
            output_buffer: vec![Sample::EQUILIBRIUM; output_buf_size * channels.get() as usize]
                .into_boxed_slice(),
            output_buffer_pos: 0,
            output_buffer_len: 0,
            channels,
            source_rate,
            input_samples_consumed: 0,
            input_exhausted: false,
            output_delay_remaining,
            total_input_frames: 0,
            total_output_samples: 0,
            expected_output_samples: 0,
            real_frames_in_buffer: 0,
            resample_ratio,
            indexing: Indexing {
                input_offset: 0,
                output_offset: 0,
                partial_len: None,
                active_channels_mask: None,
            },
        })
    }
}

/// Type alias for FFT resampler (synchronous, fixed-ratio).
/// Input and output chunk sizes must be exact multiples of the ratio components.
#[cfg(feature = "rubato-fft")]
#[cfg_attr(docsrs, doc(cfg(feature = "rubato-fft")))]
type RubatoFftResample<I> = RubatoResample<I, rubato::Fft<Sample>>;

// FFT-specific constructor
#[cfg(feature = "rubato-fft")]
#[cfg_attr(docsrs, doc(cfg(feature = "rubato-fft")))]
impl<I: Source> RubatoFftResample<I> {
    /// Create a new FFT resampler for fixed-ratio sample rate conversion.
    ///
    /// The FFT resampler requires that:
    /// - Input chunk size must be a multiple of the GCD-reduced denominator
    /// - Output chunk size must be a multiple of the GCD-reduced numerator
    fn new(
        input: I,
        target_rate: SampleRate,
        chunk_size: usize,
        sub_chunks: usize,
    ) -> Result<Self, String> {
        let source_rate = input.sample_rate();
        let channels = input.channels();

        // Calculate the GCD-reduced ratio
        let ratio = Ratio::new(target_rate.get(), source_rate.get());
        let (_num, den) = ratio.into_raw();

        // Determine input chunk size - must be multiple of denominator
        let input_chunk_size = ((chunk_size / den as usize) + 1) * den as usize;

        let resampler = rubato::Fft::new(
            source_rate.get() as usize,
            target_rate.get() as usize,
            input_chunk_size,
            sub_chunks,
            channels.get() as usize,
            rubato::FixedSync::Output,
        )
        .map_err(|e| format!("Failed to create FFT resampler: {:?}", e))?;

        let input_buf_size = resampler.input_frames_max();
        let output_buf_size = resampler.output_frames_max();
        let resample_ratio = target_rate.get() as Float / source_rate.get() as Float;

        let output_delay_remaining = Self::calculate_delay_compensation(&resampler, channels);

        Ok(Self {
            input,
            resampler,
            input_buffer: vec![Sample::EQUILIBRIUM; input_buf_size * channels.get() as usize]
                .into_boxed_slice(),
            input_frame_count: 0,
            output_buffer: vec![Sample::EQUILIBRIUM; output_buf_size * channels.get() as usize]
                .into_boxed_slice(),
            output_buffer_pos: 0,
            output_buffer_len: 0,
            channels,
            source_rate,
            input_samples_consumed: 0,
            input_exhausted: false,
            total_input_frames: 0,
            total_output_samples: 0,
            expected_output_samples: 0,
            real_frames_in_buffer: 0,
            output_delay_remaining,
            resample_ratio,
            indexing: Indexing {
                input_offset: 0,
                output_offset: 0,
                partial_len: None,
                active_channels_mask: None,
            },
        })
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
