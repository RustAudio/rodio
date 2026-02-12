//! Configuration types and builders for resampling.

use std::num::NonZero;

use crate::Float;

const DEFAULT_CHUNK_SIZE: usize = 1024;
#[cfg(feature = "rubato-fft")]
const DEFAULT_SUB_CHUNKS: usize = 1;

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
