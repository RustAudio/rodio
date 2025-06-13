//! Noise sources for audio synthesis and testing.
//!
//! ## Available Noise Types
//!
//! | **Noise Type**       | **Best For**                                            | **Sound Character**                 | **Technical Notes**                              |
//! |----------------------|---------------------------------------------------------|-------------------------------------|--------------------------------------------------|
//! | **White noise**      | Testing equipment linearly, masking sounds              | Harsh, static-like, evenly bright   | RPDF (uniform), equal power all frequencies      |
//! | **Gaussian white**   | Scientific modeling, natural processes                  | Similar to white but more natural   | GPDF (bell curve), better statistical properties |
//! | **Triangular white** | High-quality audio dithering                            | Similar to white noise              | TPDF, eliminates quantization correlation        |
//! | **Pink noise**       | Speaker testing, calibration, background sounds         | Warm, pleasant, like rainfall       | 1/f spectrum, matches human hearing              |
//! | **Blue noise**       | High-passed dithering, reducing low-frequency artifacts | Bright but smoother than white      | High-pass filtered white, less harsh             |
//! | **Violet noise**     | Testing high frequencies, harsh effects                 | Very bright, sharp, can be piercing | Heavy high-frequency emphasis                    |
//! | **Brownian noise**   | Muffled/distant effects, deep rumbles                   | Very deep, muffled, lacks highs     | Heavy low-frequency emphasis, ~5Hz cutoff        |
//! | **Velvet noise**     | Artificial reverb, room simulation                      | Sparse random impulses              | Computationally efficient, decorrelated          |
//!
//! ## Seeking Support
//!
//! **Seekable generators** (stateless or can recalculate state):
//! - White, Gaussian white, Triangular white noise (stateless)
//! - Velvet noise (can recalculate grid position from time)
//! - Violet noise (can reset differentiator state)
//!
//! **Non-seekable generators** (stateful - depends on previous samples):
//! - Pink (integrator state), Blue (depends on pink)
//! - Brownian (accumulator state)
//!
//! ## Basic Usage
//!
//! ```rust
//! use rodio::source::{white, pink, triangular_white, blue, WhiteNoise, PinkNoise, NoiseGenerator};
//! use rand::rngs::SmallRng;
//!
//! // Basic: create different noise types (all at 44.1kHz)
//! let white = white(44100);                 // For testing equipment linearly
//! let pink = pink(44100);                   // For pleasant background sound
//! let triangular = triangular_white(44100); // For TPDF dithering
//! let blue = blue(44100);                   // For high-passed dithering applications
//!
//! // Advanced: create with custom RNG (useful for deterministic output)
//! let white_custom = WhiteNoise::<SmallRng>::new_with_seed(44100, 12345);
//! let pink_custom = PinkNoise::<SmallRng>::new_with_seed(44100, 12345);
//! ```

use std::time::Duration;

use rand::{
    distr::{Distribution, Uniform},
    rngs::SmallRng,
    Rng, SeedableRng,
};
use rand_distr::{Normal, Triangular};

use crate::{ChannelCount, Sample, SampleRate, Source};

/// Trait providing common constructor patterns for noise generators.
/// Provides default implementations for `new()` and `new_with_seed()`
/// that delegate to `new_with_rng()`.
pub trait NoiseGenerator<R: Rng + SeedableRng> {
    /// Create a new noise generator with a custom RNG.
    fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self;

    /// Create a new noise generator, seeding the RNG with system entropy.
    fn new(sample_rate: SampleRate) -> Self
    where
        Self: Sized,
    {
        Self::new_with_rng(sample_rate, R::from_os_rng())
    }

    /// Create a new noise generator, seeding the RNG with `seed`.
    fn new_with_seed(sample_rate: SampleRate, seed: u64) -> Self
    where
        Self: Sized,
    {
        Self::new_with_rng(sample_rate, R::seed_from_u64(seed))
    }
}

/// Create white noise - sounds like radio static (RPDF).
/// Also known as uniform noise.
///
/// White noise has equal power at all frequencies, making it suitable for:
/// - Testing equipment frequency response linearly (flat spectrum analysis)
/// - Masking other sounds (covers all frequencies equally)
/// - As a base for creating other noise types
///
/// **Sound character**: Harsh, static-like, evenly bright across all frequencies.
/// **Distribution**: RPDF (Rectangular Probability Density Function) - uniform distribution.
///
/// ```rust
/// use rodio::source::white;
/// let white_noise = white(44100);  // 44.1kHz sample rate
/// ```
pub fn white(sample_rate: SampleRate) -> WhiteNoise<SmallRng> {
    WhiteNoise::<SmallRng>::new(sample_rate)
}

/// Create pink noise - sounds much more natural than white noise.
/// Also known as 1/f noise or flicker noise.
///
/// Pink noise emphasizes lower frequencies, making it sound more natural and pleasant than white
/// noise. Suitable for:
/// - Audio system and room calibration (industry standard, matches human hearing)
/// - Speaker and headphone testing (perceptually balanced)
/// - Pleasant background/ambient sounds
/// - Sleep sounds or concentration aids
///
/// **Sound character**: Warmer and more pleasant than white noise, like distant rainfall.
///
/// ```rust
/// use rodio::source::pink;
/// let pink_noise = pink(44100);  // Sounds much more natural than white
/// ```
pub fn pink(sample_rate: SampleRate) -> PinkNoise<SmallRng> {
    PinkNoise::<SmallRng>::new(sample_rate)
}

/// Create blue noise - sounds brighter than white noise but less harsh.
/// Also known as azure noise.
///
/// Blue noise emphasizes higher frequencies while distributing energy more evenly than white noise.
/// Suitable for:
/// - High-passed audio dithering (optimal frequency distribution - pushes noise up but not too far)
/// - Digital signal processing applications
/// - Situations where you want bright sound without harshness
/// - Reducing low-frequency rumble or artifacts
///
/// **Sound character**: Brighter than white noise but smoother and less fatiguing.
///
/// **Why not violet for dithering?** Blue noise provides the ideal balance - moves noise away from
/// audible low frequencies without pushing it so high that it causes aliasing or gets filtered out.
///
/// ```rust
/// use rodio::source::blue;
/// let blue_noise = blue(44100);  // Bright but pleasant
/// ```
pub fn blue(sample_rate: SampleRate) -> BlueNoise<SmallRng> {
    BlueNoise::<SmallRng>::new(sample_rate)
}

/// Create violet noise - very bright and sharp sounding.
/// Also known as purple noise.
///
/// Violet noise heavily emphasizes high frequencies, creating a very bright, almost piercing sound.
/// Suitable when you need:
/// - Testing high-frequency response of audio equipment
/// - Creating harsh, bright sound effects
/// - Emphasizing treble frequencies
/// - Sharp, attention-grabbing audio textures
///
/// **Sound character**: Very bright, sharp, can be harsh - use sparingly.
///
/// ```rust
/// use rodio::source::violet;
/// let violet_noise = violet(44100);  // Very bright and sharp
/// ```
pub fn violet(sample_rate: SampleRate) -> VioletNoise<SmallRng> {
    VioletNoise::<SmallRng>::new(sample_rate)
}

/// Create brownian noise - sounds very muffled and deep.
/// Also known as red noise or Brown noise.
///
/// Brownian noise heavily emphasizes low frequencies, creating a very muffled, deep sound.
/// Suitable for:
/// - Creating distant, muffled sound effects
/// - Deep, rumbling background textures
/// - Simulating sounds heard through walls or underwater
/// - Scientific modeling of random walk processes
///
/// **Sound character**: Very muffled, deep, lacks high frequencies entirely.
///
/// ```rust
/// use rodio::source::brownian;
/// let brownian_noise = brownian(44100);  // Deep and muffled
/// ```
pub fn brownian(sample_rate: SampleRate) -> BrownianNoise<SmallRng> {
    BrownianNoise::<SmallRng>::new(sample_rate)
}

/// Create velvet noise - sounds like sparse random impulses.
/// Also known as sparse noise or decorrelated noise.
///
/// Velvet noise creates random impulses with controlled spacing, not continuous noise.
/// The default density is 2000 impulses per second, which should sound smoother than white noise.
///
/// **Use for:** Artificial reverb effects, room simulation, creating decorrelated audio channels.
/// **Sound character**: Random impulses with silence between - smoother than white noise.
/// **Efficiency**: Computationally cheaper than other noise types for reverb - mostly outputs silence.
///
/// ```rust
/// use rodio::source::velvet;
/// let velvet_noise = velvet(44100);  // 2000 impulses per second
/// ```
pub fn velvet(sample_rate: SampleRate) -> VelvetNoise<SmallRng> {
    VelvetNoise::<SmallRng>::new(sample_rate)
}

/// Create Gaussian white noise - white noise with normal distribution.
/// Also known as normal noise or bell curve noise.
///
/// Has the same flat frequency spectrum as regular white noise, but uses a normal
/// (Gaussian) distribution instead of uniform. This makes it more suitable for:
/// - Scientific simulations requiring normal distribution
/// - Modeling natural random processes
/// - Applications where bell-curve statistics matter
/// - More analog-like behavior in audio modeling
///
/// **Sound character**: Very similar to regular white noise.
/// **Distribution**: GPDF (Gaussian) vs RPDF (uniform) for regular white noise.
///
/// ```rust
/// use rodio::source::gaussian_white;
/// let gaussian_white = gaussian_white(44100);  // Analog-like white noise
/// ```
pub fn gaussian_white(sample_rate: SampleRate) -> GaussianWhiteNoise<SmallRng> {
    GaussianWhiteNoise::<SmallRng>::new(sample_rate)
}

/// Create triangular white noise - optimal for high-quality dithering.
///
/// Triangular white noise uses two uniform random samples to create a triangular
/// distribution. TPDF (Triangular Probability Density Function) dithering completely
/// eliminates correlation between the original signal and quantization error, making
/// it superior to RPDF (uniform) dithering for audio applications.
///
/// **Use for:** High-quality audio dithering when reducing bit depth.
/// **Sound character**: Similar to white noise in frequency content.
/// **Distribution**: TPDF - sum of two uniform samples creates triangular probability curve.
///
/// ```rust
/// use rodio::source::triangular_white;
/// let triangular_noise = triangular_white(44100);  // Perfect for dithering
/// ```
pub fn triangular_white(sample_rate: SampleRate) -> TriangularWhiteNoise<SmallRng> {
    TriangularWhiteNoise::<SmallRng>::new(sample_rate)
}

/// Macro to implement the basic Source trait for mono noise generators.
/// This covers the common case of infinite-duration, single-channel noise.
macro_rules! impl_noise_source_basic {
    ($type:ty) => {
        impl<R: Rng> Source for $type {
            fn current_span_len(&self) -> Option<usize> {
                None
            }

            fn channels(&self) -> ChannelCount {
                1
            }

            fn sample_rate(&self) -> SampleRate {
                self.sample_rate
            }

            fn total_duration(&self) -> Option<Duration> {
                None
            }
        }
    };
}

/// Macro to implement the basic Source trait with stateless seeking support.
/// For noise generators that can seek to any position without state dependency.
macro_rules! impl_noise_source_seekable {
    ($type:ty) => {
        impl<R: Rng> Source for $type {
            fn current_span_len(&self) -> Option<usize> {
                None
            }

            fn channels(&self) -> ChannelCount {
                1
            }

            fn sample_rate(&self) -> SampleRate {
                self.sample_rate
            }

            fn total_duration(&self) -> Option<Duration> {
                None
            }

            fn try_seek(&mut self, _pos: Duration) -> Result<(), crate::source::SeekError> {
                // Stateless noise generators can seek to any position since all positions
                // are equally random and don't depend on previous state
                Ok(())
            }
        }
    };
}

/// Common sampling utilities for noise generators.
/// Provides a generic interface for sampling from any distribution.
#[derive(Clone, Debug)]
struct NoiseSampler<R: Rng, D: Distribution<f32> + Clone> {
    rng: R,
    distribution: D,
}

impl<R: Rng, D: Distribution<f32> + Clone> NoiseSampler<R, D> {
    /// Create a new sampler with the given distribution.
    fn new(rng: R, distribution: D) -> Self {
        Self { rng, distribution }
    }

    /// Generate a sample from the configured distribution.
    #[inline]
    fn sample(&mut self) -> f32 {
        self.rng.sample(&self.distribution)
    }
}

/// Generates an infinite stream of uniformly distributed white noise samples in [-1.0, 1.0].
/// White noise generator - sounds like radio static (RPDF).
///
/// Generates uniformly distributed random samples with equal power at all frequencies.
/// This is the most basic noise type and serves as a building block for other noise generators.
/// Uses RPDF (Rectangular Probability Density Function) - uniform distribution.
///
/// **When to use:** Audio equipment testing, sound masking, or as a base for other effects.
/// **Sound:** Harsh, bright, evenly distributed across all frequencies.
#[derive(Clone, Debug)]
pub struct WhiteNoise<R: Rng> {
    sample_rate: SampleRate,
    sampler: NoiseSampler<R, Uniform<f32>>,
}

impl<R: Rng + SeedableRng> NoiseGenerator<R> for WhiteNoise<R> {
    fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        let distribution =
            Uniform::new_inclusive(-1.0, 1.0).expect("Failed to create uniform distribution");
        Self {
            sample_rate,
            sampler: NoiseSampler::new(rng, distribution),
        }
    }
}

impl<R: Rng> Iterator for WhiteNoise<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.sampler.sample())
    }
}

impl_noise_source_seekable!(WhiteNoise<R>);

/// Triangular white noise generator - ideal for TPDF dithering.
///
/// Generates triangular-distributed white noise by summing two uniform random samples.
/// This creates TPDF (Triangular Probability Density Function) which is superior to
/// RPDF for audio dithering because it completely eliminates correlation between
/// the original signal and quantization error.
///
/// **When to use:** High-quality audio dithering when reducing bit depth.
/// **Sound:** Similar to white noise but with better statistical properties.
/// **Distribution**: TPDF - triangular distribution from sum of two uniform samples.
#[derive(Clone, Debug)]
pub struct TriangularWhiteNoise<R: Rng> {
    sample_rate: SampleRate,
    sampler: NoiseSampler<R, Triangular<f32>>,
}

impl<R: Rng + SeedableRng> NoiseGenerator<R> for TriangularWhiteNoise<R> {
    fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        let distribution = Triangular::new(-1.0, 1.0, 0.0).expect("Valid triangular distribution");
        Self {
            sample_rate,
            sampler: NoiseSampler::new(rng, distribution),
        }
    }
}

impl<R: Rng> Iterator for TriangularWhiteNoise<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.sampler.sample())
    }
}

impl_noise_source_seekable!(TriangularWhiteNoise<R>);

/// Velvet noise generator - creates sparse random impulses, not continuous noise.
///
/// Unlike other noise types, velvet noise produces random impulses separated
/// by periods of silence. Divides time into regular intervals and places
/// one impulse randomly within each interval.
///
/// **When to use:** Building reverb effects, room simulation, decorrelating audio channels.
/// **Sound:** Random impulses with silence between - smoother than continuous noise.
/// **Default:** 2000 impulses per second.
/// **Efficiency:** Very computationally efficient - mostly outputs zeros, only occasional computation.
#[derive(Clone, Debug)]
pub struct VelvetNoise<R: Rng> {
    sample_rate: SampleRate,
    rng: R,
    grid_size: f32,   // samples per grid cell
    grid_pos: f32,    // current position in grid cell
    impulse_pos: f32, // where impulse occurs in current grid
}

impl<R: Rng + SeedableRng> NoiseGenerator<R> for VelvetNoise<R> {
    fn new_with_rng(sample_rate: SampleRate, mut rng: R) -> Self {
        let density = 2000.0; // impulses per second
        let grid_size = sample_rate as f32 / density;
        let impulse_pos = rng.random::<f32>() * grid_size;

        Self {
            sample_rate,
            rng,
            grid_size,
            grid_pos: 0.0,
            impulse_pos,
        }
    }
}

impl<R: Rng + SeedableRng> VelvetNoise<R> {
    /// Create a new velvet noise generator with custom density (impulses per second).
    pub fn new_with_density(sample_rate: SampleRate, density: f32) -> Self {
        let mut rng = R::from_os_rng();
        let density = density.max(f32::MIN_POSITIVE);
        let grid_size = sample_rate as f32 / density;
        let impulse_pos = rng.random::<f32>() * grid_size;

        Self {
            sample_rate,
            rng,
            grid_size,
            grid_pos: 0.0,
            impulse_pos,
        }
    }
}

impl<R: Rng> Iterator for VelvetNoise<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let output = if self.grid_pos as usize == self.impulse_pos as usize {
            // Generate impulse with random polarity
            if self.rng.random::<bool>() {
                1.0
            } else {
                -1.0
            }
        } else {
            0.0
        };

        self.grid_pos += 1.0;

        // Start new grid cell when we reach the end
        if self.grid_pos >= self.grid_size {
            self.grid_pos = 0.0;
            self.impulse_pos = self.rng.random::<f32>() * self.grid_size;
        }

        Some(output)
    }
}

impl_noise_source_basic!(VelvetNoise<R>);

/// Gaussian white noise generator - statistically perfect white noise (GPDF).
///
/// Like regular white noise but with normal distribution (bell curve) instead of uniform.
/// More closely mimics analog circuits and natural processes, which typically follow bell curves.
/// Uses GPDF (Gaussian Probability Density Function) - 99.7% of samples within [-1.0, 1.0].
///
/// **When to use:** Modeling analog circuits, natural random processes, or when you need
/// more realistic noise that mimics how natural systems behave (most follow bell curves).
/// **Sound character**: Very similar to regular white noise, but with more analog-like character.
/// **vs White Noise:** Gaussian mimics natural/analog systems better, uniform white is faster and simpler.
#[derive(Clone, Debug)]
pub struct GaussianWhiteNoise<R: Rng> {
    sample_rate: SampleRate,
    sampler: NoiseSampler<R, Normal<f32>>,
}

impl<R: Rng + SeedableRng> GaussianWhiteNoise<R> {
    /// The mean of the Gaussian distribution used for sampling.
    pub fn mean(&self) -> f32 {
        self.sampler.distribution.mean()
    }

    /// The standard deviation of the Gaussian distribution used for sampling.
    pub fn std_dev(&self) -> f32 {
        self.sampler.distribution.std_dev()
    }
}

impl<R: Rng + SeedableRng> NoiseGenerator<R> for GaussianWhiteNoise<R> {
    fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        let distribution = Normal::new(0.0, 1.0 / 3.0)
            .expect("Normal distribution with mean=0, std=1/3 should be valid");
        Self {
            sample_rate,
            sampler: NoiseSampler::new(rng, distribution),
        }
    }
}

impl<R: Rng> Iterator for GaussianWhiteNoise<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Sample directly from Normal(0.0, 1/3) distribution
        // ~99.7% of samples naturally fall within [-1.0, 1.0] without clamping
        Some(self.sampler.sample())
    }
}

impl_noise_source_seekable!(GaussianWhiteNoise<R>);

/// Number of generators used in PinkNoise for frequency coverage.
///
/// The pink noise implementation uses the Voss-McCartney algorithm with 16 independent
/// generators to achieve proper 1/f frequency distribution. Each generator covers
/// approximately one octave of the frequency spectrum, providing smooth pink noise
/// characteristics across the entire audio range. 16 generators gives excellent
/// frequency coverage for sample rates from 8kHz to 192kHz+ while maintaining
/// computational efficiency.
const PINK_NOISE_GENERATORS: usize = 16;

/// Pink noise generator - sounds much more natural than white noise.
///
/// Pink noise emphasizes lower frequencies, making it sound warmer and more pleasant
/// than harsh white noise. Often described as sounding like gentle rainfall or wind.
/// Uses the industry-standard Voss-McCartney algorithm with 16 generators.
///
/// **When to use:** Audio testing (matches human hearing better), pleasant background
/// sounds, speaker testing, or any time you want "natural" sounding noise.
/// **Sound:** Warmer, more pleasant than white noise - like distant rainfall.
/// **vs White Noise:** Pink sounds much more natural and less harsh to human ears.
///
/// Technical: 1/f frequency spectrum (power decreases 3dB per octave).
/// Works correctly at all sample rates from 8kHz to 192kHz+.
#[derive(Clone, Debug)]
pub struct PinkNoise<R: Rng> {
    sample_rate: SampleRate,
    white_noise: WhiteNoise<R>,
    values: [f32; PINK_NOISE_GENERATORS],
    counters: [u32; PINK_NOISE_GENERATORS],
    max_counts: [u32; PINK_NOISE_GENERATORS],
}

impl<R: Rng + SeedableRng> NoiseGenerator<R> for PinkNoise<R> {
    fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        let mut max_counts = [1u32; PINK_NOISE_GENERATORS];
        // Each generator updates at half the rate of the previous one: 1, 2, 4, 8, 16, ...
        for i in 1..PINK_NOISE_GENERATORS {
            max_counts[i] = max_counts[i - 1] * 2;
        }

        Self {
            sample_rate,
            white_noise: WhiteNoise::new_with_rng(sample_rate, rng),
            values: [0.0; PINK_NOISE_GENERATORS],
            counters: [0; PINK_NOISE_GENERATORS],
            max_counts,
        }
    }
}

impl<R: Rng> Iterator for PinkNoise<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let mut sum = 0.0;

        // Update each generator when its counter reaches the update interval
        for i in 0..PINK_NOISE_GENERATORS {
            if self.counters[i] >= self.max_counts[i] {
                // Time to update this generator with a new white noise sample
                self.values[i] = self
                    .white_noise
                    .next()
                    .expect("WhiteNoise should never return None");
                self.counters[i] = 0;
            }
            sum += self.values[i];
            self.counters[i] += 1;
        }

        // Normalize by number of generators to keep output in reasonable range
        Some(sum / PINK_NOISE_GENERATORS as f32)
    }
}

impl_noise_source_basic!(PinkNoise<R>);

/// Blue noise generator - sounds brighter than white noise but smoother.
///
/// Blue noise emphasizes higher frequencies while distributing energy more evenly
/// than white noise. It's "brighter" sounding but less harsh and fatiguing.
/// Generated by differentiating pink noise.
///
/// **When to use:** High-passed audio dithering (preferred over violet), digital signal processing,
/// or when you want bright sound without the harshness of white noise.
/// **Sound:** Brighter than white noise but smoother and less fatiguing.
/// **vs White Noise:** Blue has better frequency distribution and less clustering.
/// **vs Violet Noise:** Blue is better for dithering - violet pushes too much energy to very high frequencies.
///
/// Technical: f frequency spectrum (power increases 3dB per octave).
#[derive(Clone, Debug)]
pub struct BlueNoise<R: Rng> {
    sample_rate: SampleRate,
    pink_noise: PinkNoise<R>,
    prev_pink: f32,
}

impl<R: Rng + SeedableRng> NoiseGenerator<R> for BlueNoise<R> {
    fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        Self {
            sample_rate,
            pink_noise: PinkNoise::new_with_rng(sample_rate, rng),
            prev_pink: 0.0,
        }
    }
}

impl<R: Rng> Iterator for BlueNoise<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let pink = self
            .pink_noise
            .next()
            .expect("PinkNoise should never return None");
        let blue = pink - self.prev_pink;
        self.prev_pink = pink;
        // Scale by 0.5 to keep output in reasonable range
        Some(blue * 0.5)
    }
}

impl_noise_source_basic!(BlueNoise<R>);

/// Violet noise generator - very bright and sharp sounding.
///
/// Violet noise (also called purple noise) heavily emphasizes high frequencies,
/// creating a very bright, sharp, sometimes harsh sound. It's the opposite of
/// brownian noise in terms of frequency emphasis.
///
/// **When to use:** Testing high-frequency equipment response, creating bright/sharp
/// sound effects, or when you need to emphasize treble frequencies.
/// **Sound:** Very bright, sharp, can be harsh - use sparingly in audio applications.
/// **vs Blue Noise:** Violet is much brighter and more aggressive than blue noise.
/// **Not ideal for dithering:** Too much energy at very high frequencies can cause aliasing.
///
/// Technical: f² frequency spectrum (power increases 6dB per octave).
/// Generated by differentiating uniform random samples.
#[derive(Clone, Debug)]
pub struct VioletNoise<R: Rng> {
    sample_rate: SampleRate,
    white_noise: WhiteNoise<R>,
    prev: f32,
}

impl<R: Rng + SeedableRng> NoiseGenerator<R> for VioletNoise<R> {
    fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        Self {
            sample_rate,
            white_noise: WhiteNoise::new_with_rng(sample_rate, rng),
            prev: 0.0, // Start at zero for consistent seeking
        }
    }
}

impl<R: Rng> Iterator for VioletNoise<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let white = self
            .white_noise
            .next()
            .expect("WhiteNoise should never return None");
        let violet = white - self.prev;
        self.prev = white;
        Some(violet)
    }
}

impl<R: Rng> Source for VioletNoise<R> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        1
    }

    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }

    fn try_seek(&mut self, _pos: Duration) -> Result<(), crate::source::SeekError> {
        // Reset differentiator state - white noise is stateless so no seeking needed
        self.prev = 0.0; // Reset to zero for consistent behavior
        Ok(())
    }
}

/// Brownian noise generator - sounds very muffled and deep.
///
/// Brownian noise (also called red noise) heavily emphasizes low frequencies,
/// creating a very muffled, deep sound with almost no high frequencies.
/// Generated by integrating Gaussian white noise with a 5Hz center frequency
/// leak factor to prevent DC buildup.
///
/// **When to use:** Creating muffled/distant effects, deep rumbling sounds,
/// or simulating sounds heard through walls or underwater.
/// **Sound:** Very muffled, deep, lacks high frequencies - sounds "distant".
/// **Technical:** Uses Gaussian white noise as input for more natural integration behavior.
#[derive(Clone, Debug)]
pub struct BrownianNoise<R: Rng> {
    sample_rate: SampleRate,
    white_noise: GaussianWhiteNoise<R>,
    accumulator: f32,
    leak_factor: f32,
    scale: f32,
}

impl<R: Rng + SeedableRng> NoiseGenerator<R> for BrownianNoise<R> {
    fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        let white_noise = GaussianWhiteNoise::new_with_rng(sample_rate, rng);

        // Leak factor prevents DC buildup while maintaining brownian characteristics.
        // Center frequency is set to 5Hz, which provides good brownian behavior
        // while preventing excessive low-frequency buildup across common sample rates.
        let center_freq_hz = 5.0;
        let leak_factor =
            1.0 - ((2.0 * std::f32::consts::PI * center_freq_hz) / sample_rate as f32);

        // Calculate the scaling factor to normalize output based on leak factor.
        // This ensures consistent output level regardless of the leak factor value.
        let stddev = white_noise.std_dev();
        let brownian_variance = (stddev * stddev) / (1.0 - leak_factor * leak_factor);
        let scale = 1.0 / brownian_variance.sqrt();

        Self {
            sample_rate,
            white_noise,
            accumulator: 0.0,
            leak_factor,
            scale,
        }
    }
}

impl<R: Rng> Iterator for BrownianNoise<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let white = self
            .white_noise
            .next()
            .expect("GaussianWhiteNoise should never return None");

        // Leaky integration: prevents DC buildup while maintaining brownian characteristics
        self.accumulator = self.accumulator * self.leak_factor + white;

        // Apply mathematically derived scaling factor for consistent output level
        Some(self.accumulator * self.scale)
    }
}

impl_noise_source_basic!(BrownianNoise<R>);
