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
//! This reduces CPU usage while providing highest quality.
//!
//! **Arbitrary ratios** (non-reducible or large fractions) use the async sinc resampler, which
//! can handle any conversion.
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
pub struct Resample<I>
where
    I: Source,
{
    inner: Option<ResampleInner<I>>,
    target_rate: SampleRate,
    config: ResampleConfig,
    cached_input_span_len: Option<usize>,
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

        if source_rate == target_rate {
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
                        RubatoResample::new_poly(source, target_rate, *chunk_size, *degree)
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
                        let resampler = RubatoResample::new_sinc(
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
                        let resampler = RubatoResample::new_sinc(
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
                        let resampler = RubatoResample::new_sinc(
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

        if ratio.is_integer() || ratio.recip().is_integer() {
            // Simple integer ratio - calculate span length directly
            let (numer, denom) = ratio.into_raw();
            input_span_len.map(|len| (len as Float * numer as Float / denom as Float) as usize)
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
enum ResampleInner<I: Source> {
    /// Passthrough when source rate is equal to the target rate
    Passthrough {
        source: I,
        input_span_pos: usize,
        channels: ChannelCount,
        source_rate: SampleRate,
    },

    /// Polynomial resampling (fast, no anti-aliasing)
    Poly(RubatoResample<I>),

    /// Sinc resampling (with anti-aliasing)
    Sinc(RubatoResample<I>),

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

/// Wrapper around Rubato's Async resampler for sample-by-sample iteration.
struct RubatoResample<I: Source> {
    input: I,
    resampler: rubato::Async<Sample>,

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

    output_delay_remaining: usize,
    resample_ratio: Float,
    indexing: Indexing,
}

/// Helper function to read one complete frame from input into the interleaved buffer.
/// Returns true if successful, false if input exhausted.
/// Sets the exhausted flag if the iterator returns None.
#[inline]
fn read_input_frame<I: Source>(
    input: &mut I,
    input_buffer: &mut [Sample],
    frame_pos: usize,
    channels: usize,
    exhausted_flag: &mut bool,
) -> bool {
    let sample_pos = frame_pos * channels;
    for ch in 0..channels {
        if let Some(sample) = input.next() {
            input_buffer[sample_pos + ch] = sample;
        } else {
            *exhausted_flag = true;
            return false;
        }
    }
    true
}

impl<I: Source> RubatoResample<I> {
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
            resample_ratio.into(),
            1.0,
            degree.into(),
            chunk_size,
            channels.get() as usize,
            rubato::FixedAsync::Output,
        )
        .map_err(|e| format!("Failed to create polynomial resampler: {:?}", e))?;

        let input_buf_size = resampler.input_frames_max();
        let output_buf_size = resampler.output_frames_max();

        // Query output delay for initial compensation
        // Skip delay-1 frames to get the first frame matching input position 0
        let delay_frames = resampler.output_delay();
        let delay_to_skip = delay_frames.saturating_sub(1);
        let output_delay_remaining = delay_to_skip * channels.get() as usize;

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
            resample_ratio.into(),
            1.0,
            &parameters,
            chunk_size,
            channels.get() as usize,
            rubato::FixedAsync::Output,
        )
        .map_err(|e| format!("Failed to create sinc resampler: {:?}", e))?;

        let input_buf_size = resampler.input_frames_max();
        let output_buf_size = resampler.output_frames_max();

        // Query output delay for initial compensation
        // Skip delay-1 frames to get the first frame matching input position 0
        let delay_frames = resampler.output_delay();
        let delay_to_skip = delay_frames.saturating_sub(1);
        let output_delay_remaining = delay_to_skip * channels.get() as usize;

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
            resample_ratio,
            indexing: Indexing {
                input_offset: 0,
                output_offset: 0,
                partial_len: None,
                active_channels_mask: None,
            },
        })
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
    }

    fn next_sample(&mut self) -> Option<Sample> {
        let num_channels = self.channels.get() as usize;
        loop {
            // If we have buffered output, return it
            if self.output_buffer_pos < self.output_buffer_len {
                let sample = self.output_buffer[self.output_buffer_pos];
                self.output_buffer_pos += 1;

                // Skip warmup delay samples
                if self.output_delay_remaining > 0 {
                    self.output_delay_remaining -= 1;
                    continue;
                }

                self.total_output_samples = self.total_output_samples.saturating_add(1);

                if self.input_exhausted {
                    let expected_output_frames =
                        (self.total_input_frames as Float * self.resample_ratio).ceil() as usize;
                    let expected_output_samples = expected_output_frames * num_channels;
                    if self.total_output_samples > expected_output_samples {
                        // Cut off filter artifacts after input is exhausted
                        return None;
                    }
                }

                return Some(sample);
            }

            // Need more input - first check if we're completely done
            if self.input_exhausted && self.input_frame_count == 0 {
                return None;
            }

            // Fill input buffer - accumulate frames until we hit needed amount or run out of input
            let needed_input = self.resampler.input_frames_next();
            let frames_before = self.input_frame_count;
            while self.input_frame_count < needed_input && !self.input_exhausted {
                if read_input_frame(
                    &mut self.input,
                    &mut self.input_buffer,
                    self.input_frame_count,
                    num_channels,
                    &mut self.input_exhausted,
                ) {
                    self.input_frame_count += 1;
                } else {
                    break;
                }
            }

            // If we have no input to process, we're done
            if self.input_frame_count == 0 {
                return None;
            }

            // For FixedAsync::Output, we can process with fewer frames than needed using
            // partial_len when the input is exhausted. If we don't have enough input and
            // more is coming, wait.
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
            self.total_input_frames += actual_consumed;

            // Shift remaining input samples to beginning of buffer
            if actual_consumed < self.input_frame_count {
                let src_start = actual_consumed * num_channels;
                let src_end = self.input_frame_count * num_channels;
                self.input_buffer.copy_within(src_start..src_end, 0);
            }
            self.input_frame_count -= actual_consumed;

            self.output_buffer_pos = 0;
            self.output_buffer_len = frames_out * num_channels;
        }
    }
}

/// Wrapper around Rubato's FFT resampler for sample-by-sample iteration.
///
/// The FFT resampler is synchronous and optimal for fixed sample rate conversions.
/// Input and output chunk sizes must be exact multiples of the ratio components.
#[cfg(feature = "rubato-fft")]
#[cfg_attr(docsrs, doc(cfg(feature = "rubato-fft")))]
struct RubatoFftResample<I: Source> {
    input: I,
    resampler: rubato::Fft<Sample>,

    input_buffer: Box<[Sample]>,
    input_frame_count: usize,

    output_buffer: Box<[Sample]>,
    output_buffer_pos: usize,
    output_buffer_len: usize,

    channels: ChannelCount,
    source_rate: SampleRate,

    input_samples_consumed: usize,
    input_exhausted: bool,
}

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
        })
    }

    fn reset(&mut self) {
        self.resampler.reset();
        self.output_buffer_pos = 0;
        self.output_buffer_len = 0;
        self.input_frame_count = 0;
        self.input_samples_consumed = 0;
        self.input_exhausted = false;
    }

    fn next_sample(&mut self) -> Option<Sample> {
        loop {
            // If we have buffered output, return it (interleaved format)
            if self.output_buffer_pos < self.output_buffer_len {
                let sample = self.output_buffer[self.output_buffer_pos];
                self.output_buffer_pos += 1;
                return Some(sample);
            }

            // Need more output - first check if we're completely done
            if self.input_exhausted && self.input_frame_count == 0 {
                return None;
            }

            // For FFT synchronous resampler, we need exactly the right number of input frames
            let needed_input = self.resampler.input_frames_next();

            // Fill input buffer
            let frames_before = self.input_frame_count;
            while self.input_frame_count < needed_input && !self.input_exhausted {
                if read_input_frame(
                    &mut self.input,
                    &mut self.input_buffer,
                    self.input_frame_count,
                    self.channels.get() as usize,
                    &mut self.input_exhausted,
                ) {
                    self.input_frame_count += 1;
                } else {
                    break;
                }
            }

            // FFT resampler requires exact chunk sizes
            // If we have less than needed and input is exhausted, we need to pad with zeros
            // Only continue if we made progress reading frames
            let made_progress = self.input_frame_count > frames_before;
            if self.input_frame_count < needed_input {
                if !self.input.is_exhausted() && made_progress {
                    continue; // Wait for more input
                }

                // Pad remaining frames with zeros (interleaved)
                let num_channels = self.channels.get() as usize;
                let start_sample = self.input_frame_count * num_channels;
                let end_sample = needed_input * num_channels;
                for i in start_sample..end_sample {
                    self.input_buffer[i] = 0.0;
                }
                self.input_frame_count = needed_input;
            }

            let num_channels = self.channels.get() as usize;

            // Create input adapter from interleaved buffer
            let input_adapter = match audioadapter_buffers::direct::InterleavedSlice::new(
                &self.input_buffer,
                num_channels,
                needed_input,
            ) {
                Ok(adapter) => adapter,
                Err(_) => return None,
            };

            // Create output adapter wrapping our interleaved buffer
            // Rubato will write directly to our interleaved format via the adapter
            let output_frames = self.output_buffer.len() / num_channels;
            let mut output_adapter = match audioadapter_buffers::direct::InterleavedSlice::new_mut(
                &mut self.output_buffer,
                num_channels,
                output_frames,
            ) {
                Ok(adapter) => adapter,
                Err(_) => return None,
            };

            // FFT resampler uses None for indexing (exact chunk sizes)
            let result =
                self.resampler
                    .process_into_buffer(&input_adapter, &mut output_adapter, None);

            let (frames_in, frames_out) = match result {
                Ok((input_consumed, output_produced)) => (input_consumed, output_produced),
                Err(_) => return None,
            };

            // Track input samples consumed for span boundary detection
            self.input_samples_consumed += frames_in * num_channels;

            // Clear input buffer after processing
            self.input_frame_count = self.input_frame_count.saturating_sub(frames_in);

            // Reset output position and set buffer length
            self.output_buffer_pos = 0;
            // This is the TOTAL length of the current output span (in samples, not frames)
            self.output_buffer_len = frames_out * num_channels;

            if frames_out == 0 && self.input_exhausted {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Source;

    struct TestSource {
        samples: Vec<Sample>,
        index: usize,
        sample_rate: u32,
        channels: u16,
    }

    impl TestSource {
        fn new(samples: Vec<Sample>, sample_rate: u32, channels: u16) -> Self {
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
            SampleRate::new(self.sample_rate).unwrap()
        }

        fn channels(&self) -> ChannelCount {
            ChannelCount::new(self.channels).unwrap()
        }

        fn total_duration(&self) -> Option<Duration> {
            let samples = self.samples.len() / self.channels as usize;
            Some(Duration::from_secs_f64(
                samples as f64 / self.sample_rate as f64,
            ))
        }

        fn try_seek(&mut self, _position: Duration) -> Result<(), SeekError> {
            Ok(())
        }
    }

    #[test]
    fn test_passthrough_same_rate() {
        // When source rate equals target rate, should passthrough bit-perfectly
        let samples = vec![1.0, 2.0, 3.0, 4.0];
        let source = TestSource::new(samples.clone(), 44100, 1);
        let target_rate = SampleRate::new(44100).unwrap();
        let config = ResampleConfig::balanced();
        let mut resampled = Resample::new(source, target_rate, config);

        // Should get exact same samples out (bit-perfect passthrough)
        for expected in samples {
            assert_eq!(resampled.next(), Some(expected));
        }
        assert_eq!(resampled.next(), None);
    }

    #[test]
    fn test_resample_config_builder() {
        use std::num::NonZero;
        let config = ResampleConfig::sinc()
            .sinc_len(NonZero::new(256).unwrap())
            .chunk_size(NonZero::new(512).unwrap())
            .window(WindowFunction::Hann2)
            .build();

        // Check that it's a Sinc config with the right parameters
        match config {
            ResampleConfig::Sinc {
                chunk_size,
                window,
                sinc_len,
                ..
            } => {
                assert_eq!(chunk_size, 512);
                assert_eq!(window, WindowFunction::Hann2);
                assert_eq!(sinc_len, 256);
            }
            _ => panic!("Expected Sinc config"),
        }
    }

    #[test]
    fn test_upsample_linear() {
        // Test upsampling from 44100 to 88200 (2x)
        let samples = vec![0.0, 0.5, 1.0, 0.5, 0.0, -0.5, -1.0, -0.5];
        let source = TestSource::new(samples, 44100, 1);
        let target_rate = SampleRate::new(88200).unwrap();
        let config = ResampleConfig::poly().degree(Poly::Linear).build();
        let resampled = Resample::new(source, target_rate, config);

        // Should produce more samples (approximately 2x)
        let output: Vec<_> = resampled.take(20).collect();
        assert!(
            output.len() > 10,
            "Expected at least 10 samples from upsampling, got {}",
            output.len()
        );
    }

    #[test]
    fn test_downsample_sinc() {
        // Test downsampling from 88200 to 44100 (0.5x)
        // Use 1000 samples to account for sinc filter latency (128 taps for Balanced quality)
        let samples = vec![1.0; 1000];
        let source = TestSource::new(samples, 88200, 1);
        let target_rate = SampleRate::new(44100).unwrap();
        let config = ResampleConfig::balanced();
        let resampled = Resample::new(source, target_rate, config);

        // Should produce approximately 0.5x samples (500), but with filter latency
        // The sinc filter can add latency that results in slightly more or fewer output samples
        let output: Vec<_> = resampled.collect();
        assert!(
            output.len() < 1100,
            "Expected fewer than 1100 samples from downsampling, got {}",
            output.len()
        );
        assert!(
            output.len() > 300,
            "Expected at least 300 samples, got {}",
            output.len()
        );
    }

    #[test]
    fn test_resample_stereo() {
        // Test stereo resampling
        let samples = vec![1.0, -1.0, 0.5, -0.5, 0.0, 0.0, -0.5, 0.5];
        let source = TestSource::new(samples, 44100, 2);
        let target_rate = SampleRate::new(48000).unwrap();
        let config = ResampleConfig::fast();
        let resampled = Resample::new(source, target_rate, config);

        // Should produce samples
        let output: Vec<_> = resampled.take(10).collect();
        assert!(!output.is_empty(), "Should produce output samples");
    }

    #[test]
    fn test_standard_sample_rate_conversions() {
        // Test common sample rate conversions
        let test_cases = [
            (44100, 48000), // CD to DVD
            (48000, 44100), // DVD to CD
            (44100, 96000), // CD to high-res
            (96000, 48000), // High-res to DVD
        ];

        for (from_rate, to_rate) in test_cases {
            let samples: Vec<Sample> = vec![0.5; 500];
            let source = TestSource::new(samples, from_rate, 1);
            let target_rate = SampleRate::new(to_rate).unwrap();
            let config = ResampleConfig::fast();
            let resampled = Resample::new(source, target_rate, config);

            let output: Vec<_> = resampled.collect();
            let expected_approx = (500.0 * to_rate as f64 / from_rate as f64) as usize;

            // Allow some tolerance for filter latency
            assert!(
                output.len() > expected_approx / 2,
                "Conversion {}Hz -> {}Hz: expected ~{} samples, got {}",
                from_rate,
                to_rate,
                expected_approx,
                output.len()
            );
        }
    }

    // FFT-specific tests - only run when rubato-fft feature is enabled
    #[cfg(feature = "rubato-fft")]
    mod fft_tests {
        use super::*;

        #[test]
        fn test_fft_resampler_cd_to_dvd() {
            // Test FFT resampling from 44100 to 48000 (147:160)
            // This is a common fixed-ratio conversion ideal for FFT
            let samples: Vec<Sample> = (0..2000).map(|i| (i as Sample * 0.05).sin()).collect();

            let source = TestSource::new(samples, 44100, 1);
            let target_rate = SampleRate::new(48000).unwrap();
            let config = ResampleConfig::balanced();
            let resampled = Resample::new(source, target_rate, config);

            // Should produce approximately 48000/44100 * 2000 ≈ 2177 samples
            let output: Vec<_> = resampled.collect();
            assert!(
                output.len() > 2000,
                "Should produce more samples (upsampling): got {}",
                output.len()
            );
            assert!(
                output.len() < 2500,
                "Should not produce too many samples: got {}",
                output.len()
            );
        }

        #[test]
        fn test_fft_resampler_stereo() {
            // Test FFT resampling with stereo audio
            let samples: Vec<Sample> = (0..2000)
                .flat_map(|i| {
                    let left = (i as Sample * 0.05).sin();
                    let right = (i as Sample * 0.05).cos();
                    [left, right]
                })
                .collect();

            let source = TestSource::new(samples, 44100, 2);
            let target_rate = SampleRate::new(48000).unwrap();
            let config = ResampleConfig::fast();
            let resampled = Resample::new(source, target_rate, config);

            // Should produce samples for both channels
            let output: Vec<_> = resampled.collect();
            // Output should be multiple of 2 (stereo)
            assert_eq!(
                output.len() % 2,
                0,
                "Stereo output should have even number of samples"
            );
            assert!(!output.is_empty(), "Should produce output samples");
        }
    }
}
