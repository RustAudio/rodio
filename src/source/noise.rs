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
//! ## Basic Usage
//!
//! ```rust
//! use rodio::source::noise::{WhiteUniform, Pink, WhiteTriangular, Blue};
//!
//! // Simple usage - creates generators with `SmallRng`
//! let white = WhiteUniform::new(44100);          // For testing equipment linearly
//! let pink = Pink::new(44100);                   // For pleasant background sound
//! let triangular = WhiteTriangular::new(44100);  // For TPDF dithering
//! let blue = Blue::new(44100);                   // For high-passed dithering applications
//!
//! // Advanced usage - specify your own RNG type
//! use rand::{rngs::StdRng, SeedableRng};
//! let white_custom = WhiteUniform::<StdRng>::new_with_rng(44100, StdRng::seed_from_u64(12345));
//! ```

use std::time::Duration;

use rand::{
    distr::{Distribution, Uniform},
    rngs::SmallRng,
    Rng, SeedableRng,
};
use rand_distr::{Normal, Triangular};

use crate::{ChannelCount, Sample, SampleRate, Source};

/// Convenience function to create a new `WhiteUniform` noise source.
#[deprecated(since = "0.21", note = "use WhiteUniform::new() instead")]
pub fn white(sample_rate: SampleRate) -> WhiteUniform<SmallRng> {
    WhiteUniform::new(sample_rate)
}

/// Convenience function to create a new `Pink` noise source.
#[deprecated(since = "0.21", note = "use Pink::new() instead")]
pub fn pink(sample_rate: SampleRate) -> Pink<SmallRng> {
    Pink::new(sample_rate)
}

/// Macro to implement the basic `Source` trait for mono noise generators with stateless seeking
/// support.
macro_rules! impl_noise_source {
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
pub struct WhiteUniform<R: Rng = SmallRng> {
    sample_rate: SampleRate,
    sampler: NoiseSampler<R, Uniform<f32>>,
}

impl WhiteUniform<SmallRng> {
    /// Create a new white noise generator with `SmallRng` seeded from system entropy.
    pub fn new(sample_rate: SampleRate) -> Self {
        Self::new_with_rng(sample_rate, SmallRng::from_os_rng())
    }
}

impl<R: Rng + SeedableRng> WhiteUniform<R> {
    /// Create a new white noise generator with a custom RNG.
    pub fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        let distribution =
            Uniform::new_inclusive(-1.0, 1.0).expect("Failed to create uniform distribution");

        Self {
            sample_rate,
            sampler: NoiseSampler::new(rng, distribution),
        }
    }
}

impl<R: Rng> Iterator for WhiteUniform<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.sampler.sample())
    }
}

impl_noise_source!(WhiteUniform<R>);

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
pub struct WhiteTriangular<R: Rng = SmallRng> {
    sample_rate: SampleRate,
    sampler: NoiseSampler<R, Triangular<f32>>,
}

impl WhiteTriangular<SmallRng> {
    /// Create a new triangular white noise generator with SmallRng seeded from system entropy.
    pub fn new(sample_rate: SampleRate) -> Self {
        Self::new_with_rng(sample_rate, SmallRng::from_os_rng())
    }
}

impl<R: Rng + SeedableRng> WhiteTriangular<R> {
    /// Create a new triangular white noise generator with a custom RNG.
    pub fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        let distribution = Triangular::new(-1.0, 1.0, 0.0).expect("Valid triangular distribution");

        Self {
            sample_rate,
            sampler: NoiseSampler::new(rng, distribution),
        }
    }
}

impl<R: Rng> Iterator for WhiteTriangular<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.sampler.sample())
    }
}

impl_noise_source!(WhiteTriangular<R>);

/// Velvet noise generator - creates sparse random impulses, not continuous noise.
/// Also known as sparse noise or decorrelated noise.
///
/// Unlike other noise types, velvet noise produces random impulses separated
/// by periods of silence. Divides time into regular intervals and places
/// one impulse randomly within each interval.
///
/// **When to use:** Building reverb effects, room simulation, decorrelating audio channels.
/// **Sound:** Random impulses with silence between - smoother than continuous noise.
/// **Default:** 2000 impulses per second.
/// **Efficiency:** Very computationally efficient - mostly outputs zeros, only occasional
/// computation.
#[derive(Clone, Debug)]
pub struct Velvet<R: Rng = SmallRng> {
    sample_rate: SampleRate,
    rng: R,
    grid_size: f32,   // samples per grid cell
    grid_pos: f32,    // current position in grid cell
    impulse_pos: f32, // where impulse occurs in current grid
}

impl Velvet<SmallRng> {
    /// Create a new velvet noise generator with SmallRng seeded from system entropy.
    pub fn new(sample_rate: SampleRate) -> Self {
        Self::new_with_rng(sample_rate, SmallRng::from_os_rng())
    }
}

impl<R: Rng + SeedableRng> Velvet<R> {
    /// Create a new velvet noise generator with a custom RNG.
    pub fn new_with_rng(sample_rate: SampleRate, mut rng: R) -> Self {
        let density = VELVET_DEFAULT_DENSITY;
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

impl<R: Rng + SeedableRng> Velvet<R> {
    /// Create a new velvet noise generator with custom density (impulses per second).
    ///
    /// **Density guidelines:**
    /// - 500-1000 Hz: Sparse, distant reverb effects
    /// - 1000-2000 Hz: Balanced reverb simulation (default: 2000 Hz)
    /// - 2000-4000 Hz: Dense, close reverb effects
    /// - >4000 Hz: Very dense, approaching continuous noise
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

impl<R: Rng> Iterator for Velvet<R> {
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

impl_noise_source!(Velvet<R>);

/// Gaussian white noise generator - statistically perfect white noise (GPDF).
/// Also known as normal noise or bell curve noise.
///
/// Like regular white noise but with normal distribution (bell curve) instead of uniform.
/// More closely mimics analog circuits and natural processes, which typically follow bell curves.
/// Uses GPDF (Gaussian Probability Density Function) - 99.7% of samples within [-1.0, 1.0].
///
/// **When to use:** Modeling analog circuits, natural random processes, or when you need
/// more realistic noise that mimics how natural systems behave (most follow bell curves).
/// **Sound character**: Very similar to regular white noise, but with more analog-like character.
/// **vs White Noise:** Gaussian mimics natural/analog systems better, uniform white is faster and simpler.
/// **Clipping Warning:** Can rarely exceed [-1.0, 1.0] bounds (~0.3% of samples). Consider attenuation or limiting if clipping is critical.
#[derive(Clone, Debug)]
pub struct WhiteGaussian<R: Rng = SmallRng> {
    sample_rate: SampleRate,
    sampler: NoiseSampler<R, Normal<f32>>,
}

impl<R: Rng + SeedableRng> WhiteGaussian<R> {
    /// Get the mean (average) value of the noise distribution.
    pub fn mean(&self) -> f32 {
        self.sampler.distribution.mean()
    }

    /// Get the standard deviation of the noise distribution.
    pub fn std_dev(&self) -> f32 {
        self.sampler.distribution.std_dev()
    }
}

impl WhiteGaussian<SmallRng> {
    /// Create a new Gaussian white noise generator with `SmallRng` seeded from system entropy.
    pub fn new(sample_rate: SampleRate) -> Self {
        Self::new_with_rng(sample_rate, SmallRng::from_os_rng())
    }
}

impl<R: Rng + SeedableRng> WhiteGaussian<R> {
    /// Create a new Gaussian white noise generator with a custom RNG.
    pub fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        let distribution = Normal::new(0.0, 1.0 / 3.0)
            .expect("Normal distribution with mean=0, std=1/3 should be valid");

        Self {
            sample_rate,
            sampler: NoiseSampler::new(rng, distribution),
        }
    }
}

impl<R: Rng> Iterator for WhiteGaussian<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Sample directly from Normal(0.0, 1/3) distribution and clamp to [-1.0, 1.0]
        // This ensures all samples are bounded as required
        Some(self.sampler.sample().clamp(-1.0, 1.0))
    }
}

impl_noise_source!(WhiteGaussian<R>);

/// Number of generators used in PinkNoise for frequency coverage.
///
/// The pink noise implementation uses the Voss-McCartney algorithm with 16 independent
/// generators to achieve proper 1/f frequency distribution. Each generator covers
/// approximately one octave of the frequency spectrum, providing smooth pink noise
/// characteristics across the entire audio range. 16 generators gives excellent
/// frequency coverage for sample rates from 8kHz to 192kHz+ while maintaining
/// computational efficiency.
const PINK_NOISE_GENERATORS: usize = 16;

/// Default impulse density for Velvet noise in impulses per second.
///
/// This provides a good balance between realistic reverb characteristics and
/// computational efficiency. Lower values create sparser, more distant reverb
/// effects, while higher values create denser, closer reverb simulation.
const VELVET_DEFAULT_DENSITY: f32 = 2000.0;

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
pub struct Pink<R: Rng = SmallRng> {
    sample_rate: SampleRate,
    white_noise: WhiteUniform<R>,
    values: [f32; PINK_NOISE_GENERATORS],
    counters: [u32; PINK_NOISE_GENERATORS],
    max_counts: [u32; PINK_NOISE_GENERATORS],
}

impl Pink<SmallRng> {
    /// Create a new pink noise generator with `SmallRng` seeded from system entropy.
    pub fn new(sample_rate: SampleRate) -> Self {
        Self::new_with_rng(sample_rate, SmallRng::from_os_rng())
    }
}

impl<R: Rng + SeedableRng> Pink<R> {
    /// Create a new pink noise generator with a custom RNG.
    pub fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        let mut max_counts = [1u32; PINK_NOISE_GENERATORS];
        // Each generator updates at half the rate of the previous one: 1, 2, 4, 8, 16, ...
        for i in 1..PINK_NOISE_GENERATORS {
            max_counts[i] = max_counts[i - 1] * 2;
        }

        Self {
            sample_rate,
            white_noise: WhiteUniform::new_with_rng(sample_rate, rng),
            values: [0.0; PINK_NOISE_GENERATORS],
            counters: [0; PINK_NOISE_GENERATORS],
            max_counts,
        }
    }
}

impl<R: Rng> Iterator for Pink<R> {
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

impl_noise_source!(Pink<R>);

/// Blue noise generator - sounds brighter than white noise but smoother.
///
/// Blue noise emphasizes higher frequencies while distributing energy more evenly
/// than white noise. It's "brighter" sounding but less harsh and fatiguing.
/// Generated by differentiating pink noise. Also known as azure noise.
///
/// **When to use:** High-passed audio dithering (preferred over violet), digital signal processing,
/// or when you want bright sound without the harshness of white noise.
/// **Sound:** Brighter than white noise but smoother and less fatiguing.
/// **vs White Noise:** Blue has better frequency distribution and less clustering.
/// **vs Violet Noise:** Blue is better for dithering - violet pushes too much energy to very high frequencies.
/// **Clipping Warning:** Can exceed [-1.0, 1.0] bounds due to differentiation. Consider attenuation or limiting if clipping is critical.
///
/// Technical: f frequency spectrum (power increases 3dB per octave).
#[derive(Clone, Debug)]
pub struct Blue<R: Rng = SmallRng> {
    sample_rate: SampleRate,
    white_noise: WhiteGaussian<R>,
    prev_white: f32,
}

impl Blue<SmallRng> {
    /// Create a new blue noise generator with `SmallRng` seeded from system entropy.
    pub fn new(sample_rate: SampleRate) -> Self {
        Self::new_with_rng(sample_rate, SmallRng::from_os_rng())
    }
}

impl<R: Rng + SeedableRng> Blue<R> {
    /// Create a new blue noise generator with a custom RNG.
    pub fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        Self {
            sample_rate,
            white_noise: WhiteGaussian::new_with_rng(sample_rate, rng),
            prev_white: 0.0,
        }
    }
}

impl<R: Rng> Iterator for Blue<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let white = self
            .white_noise
            .next()
            .expect("White noise should never return None");
        let blue = white - self.prev_white;
        self.prev_white = white;
        Some(blue)
    }
}

impl_noise_source!(Blue<R>);

/// Violet noise generator - very bright and sharp sounding.
/// Also known as purple noise.
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
/// **Clipping Warning:** Can exceed [-1.0, 1.0] bounds due to differentiation. Consider attenuation or limiting if clipping is critical.
///
/// Technical: f² frequency spectrum (power increases 6dB per octave).
/// Generated by differentiating uniform random samples.
#[derive(Clone, Debug)]
pub struct Violet<R: Rng = SmallRng> {
    sample_rate: SampleRate,
    blue_noise: Blue<R>,
    prev: f32,
}

impl Violet<SmallRng> {
    /// Create a new violet noise generator with `SmallRng` seeded from system entropy.
    pub fn new(sample_rate: SampleRate) -> Self {
        Self::new_with_rng(sample_rate, SmallRng::from_os_rng())
    }
}

impl<R: Rng + SeedableRng> Violet<R> {
    /// Create a new violet noise generator with a custom RNG.
    pub fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        Self {
            sample_rate,
            blue_noise: Blue::new_with_rng(sample_rate, rng),
            prev: 0.0,
        }
    }
}

impl<R: Rng> Iterator for Violet<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let blue = self
            .blue_noise
            .next()
            .expect("Blue noise should never return None");
        let violet = blue - self.prev; // Difference can exceed [-1.0, 1.0] - this is mathematically correct
        self.prev = blue;
        Some(violet)
    }
}

impl_noise_source!(Violet<R>);

/// Brownian noise generator - sounds very muffled and deep.
/// Also known as red noise or Brown noise.
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
/// **Clipping Warning:** Can exceed [-1.0, 1.0] bounds due to integration. Consider attenuation or limiting if clipping is critical.
#[derive(Clone, Debug)]
pub struct Brownian<R: Rng = SmallRng> {
    sample_rate: SampleRate,
    white_noise: WhiteGaussian<R>,
    accumulator: f32,
    leak_factor: f32,
    scale: f32,
}

impl Brownian<SmallRng> {
    /// Create a new brownian noise generator with `SmallRng` seeded from system entropy.
    pub fn new(sample_rate: SampleRate) -> Self {
        Self::new_with_rng(sample_rate, SmallRng::from_os_rng())
    }
}

impl<R: Rng + SeedableRng> Brownian<R> {
    /// Create a new brownian noise generator with a custom RNG.
    pub fn new_with_rng(sample_rate: SampleRate, rng: R) -> Self {
        let white_noise = WhiteGaussian::new_with_rng(sample_rate, rng);

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

impl<R: Rng> Iterator for Brownian<R> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let white = self
            .white_noise
            .next()
            .expect("GaussianWhiteNoise should never return None");

        // Leaky integration: prevents DC buildup while maintaining brownian characteristics
        self.accumulator = self.accumulator * self.leak_factor + white;
        Some(self.accumulator * self.scale)
    }
}

impl_noise_source!(Brownian<R>);

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::SmallRng;
    use rand::SeedableRng;
    use rstest::rstest;
    use rstest_reuse::{self, *};

    // Test constants
    const TEST_SAMPLE_RATE: u32 = 44100;
    const TEST_SAMPLES_SMALL: usize = 100;
    const TEST_SAMPLES_MEDIUM: usize = 1000;

    // Helper function to create iterator from generator name
    fn create_generator_iterator(name: &str) -> Box<dyn Iterator<Item = f32>> {
        match name {
            "WhiteUniform" => Box::new(WhiteUniform::new(TEST_SAMPLE_RATE)),
            "WhiteTriangular" => Box::new(WhiteTriangular::new(TEST_SAMPLE_RATE)),
            "WhiteGaussian" => Box::new(WhiteGaussian::new(TEST_SAMPLE_RATE)),
            "Pink" => Box::new(Pink::new(TEST_SAMPLE_RATE)),
            "Blue" => Box::new(Blue::new(TEST_SAMPLE_RATE)),
            "Violet" => Box::new(Violet::new(TEST_SAMPLE_RATE)),
            "Brownian" => Box::new(Brownian::new(TEST_SAMPLE_RATE)),
            "Velvet" => Box::new(Velvet::new(TEST_SAMPLE_RATE)),
            _ => panic!("Unknown generator: {name}"),
        }
    }

    // Helper function to create source from generator name
    fn create_generator_source(name: &str) -> Box<dyn Source> {
        match name {
            "WhiteUniform" => Box::new(WhiteUniform::new(TEST_SAMPLE_RATE)),
            "WhiteTriangular" => Box::new(WhiteTriangular::new(TEST_SAMPLE_RATE)),
            "WhiteGaussian" => Box::new(WhiteGaussian::new(TEST_SAMPLE_RATE)),
            "Pink" => Box::new(Pink::new(TEST_SAMPLE_RATE)),
            "Blue" => Box::new(Blue::new(TEST_SAMPLE_RATE)),
            "Violet" => Box::new(Violet::new(TEST_SAMPLE_RATE)),
            "Brownian" => Box::new(Brownian::new(TEST_SAMPLE_RATE)),
            "Velvet" => Box::new(Velvet::new(TEST_SAMPLE_RATE)),
            _ => panic!("Unknown generator: {name}"),
        }
    }

    // Templates for different generator groups
    #[template]
    #[rstest]
    #[case("WhiteUniform")]
    #[case("WhiteTriangular")]
    #[case("WhiteGaussian")]
    #[case("Pink")]
    #[case("Blue")]
    #[case("Violet")]
    #[case("Brownian")]
    #[case("Velvet")]
    fn all_generators(#[case] generator_name: &str) {}

    // Generators that are mathematically bounded to [-1.0, 1.0]
    #[template]
    #[rstest]
    #[case("WhiteUniform")]
    #[case("WhiteTriangular")]
    #[case("Pink")]
    #[case("Velvet")]
    fn bounded_generators(#[case] generator_name: &str) {}

    // Generators that can mathematically exceed [-1.0, 1.0] (differentiators and integrators)
    #[template]
    #[rstest]
    #[case("WhiteGaussian")] // Gaussian can exceed bounds (3-sigma rule, ~0.3% chance)
    #[case("Blue")] // Difference of bounded values can exceed bounds
    #[case("Violet")] // Difference of bounded values can exceed bounds
    #[case("Brownian")] // Integration can exceed bounds despite scaling
    fn unbounded_generators(#[case] generator_name: &str) {}

    // Test that mathematically bounded generators stay within [-1.0, 1.0]
    #[apply(bounded_generators)]
    #[trace]
    fn test_bounded_generators_range(generator_name: &str) {
        let mut generator = create_generator_iterator(generator_name);
        for i in 0..TEST_SAMPLES_MEDIUM {
            let sample = generator.next().unwrap();
            assert!(
                (-1.0..=1.0).contains(&sample),
                "{generator_name} sample {i} out of range [-1.0, 1.0]: {sample}"
            );
        }
    }

    // Test that unbounded generators produce finite samples (no bounds check)
    #[apply(unbounded_generators)]
    #[trace]
    fn test_unbounded_generators_finite(generator_name: &str) {
        let mut generator = create_generator_iterator(generator_name);
        for i in 0..TEST_SAMPLES_MEDIUM {
            let sample = generator.next().unwrap();
            assert!(
                sample.is_finite(),
                "{generator_name} produced non-finite sample at index {i}: {sample}"
            );
        }
    }

    // Test that generators can seek without errors
    #[apply(all_generators)]
    #[trace]
    fn test_generators_seek(generator_name: &str) {
        let mut generator = create_generator_source(generator_name);
        let seek_result = generator.try_seek(std::time::Duration::from_secs(1));
        assert!(
            seek_result.is_ok(),
            "{generator_name} should support seeking but returned error: {seek_result:?}"
        );
    }

    // Test common Source trait properties for all generators
    #[apply(all_generators)]
    #[trace]
    fn test_source_trait_properties(generator_name: &str) {
        let source = create_generator_source(generator_name);

        // All noise generators should be mono (1 channel)
        assert_eq!(source.channels(), 1, "{generator_name} should be mono");

        // All should have the expected sample rate
        assert_eq!(
            source.sample_rate(),
            TEST_SAMPLE_RATE,
            "{generator_name} should have correct sample rate"
        );

        // All should have infinite duration
        assert_eq!(
            source.total_duration(),
            None,
            "{generator_name} should have infinite duration"
        );

        // All should return None for current_span_len (infinite streams)
        assert_eq!(
            source.current_span_len(),
            None,
            "{generator_name} should have no span length limit"
        );
    }

    #[test]
    fn test_white_uniform_distribution() {
        let mut generator = WhiteUniform::new(TEST_SAMPLE_RATE);
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;

        for _ in 0..TEST_SAMPLES_MEDIUM {
            let sample = generator.next().unwrap();
            min = min.min(sample);
            max = max.max(sample);
        }

        // Should use the full range approximately
        assert!(min < -0.9, "Min sample should be close to -1.0: {min}");
        assert!(max > 0.9, "Max sample should be close to 1.0: {max}");
    }

    #[test]
    fn test_triangular_distribution() {
        let mut generator = WhiteTriangular::new(TEST_SAMPLE_RATE);

        // Triangular distribution should have most values near 0
        let mut near_zero_count = 0;
        let total_samples = TEST_SAMPLES_MEDIUM;

        for _ in 0..total_samples {
            let sample = generator.next().unwrap();
            if sample.abs() < 0.5 {
                near_zero_count += 1;
            }
        }

        // Triangular distribution should have more samples near zero than uniform
        assert!(
            near_zero_count > total_samples / 2,
            "Triangular distribution should favor values near zero"
        );
    }

    #[test]
    fn test_gaussian_noise_properties() {
        let generator = WhiteGaussian::new(TEST_SAMPLE_RATE);
        assert_eq!(generator.std_dev(), 1.0 / 3.0);
        assert_eq!(generator.mean(), 0.0);

        // Test that most samples fall within 3 standard deviations (should be ~99.7%)
        let mut generator = WhiteGaussian::new(TEST_SAMPLE_RATE);
        let samples: Vec<f32> = (0..TEST_SAMPLES_MEDIUM)
            .map(|_| generator.next().unwrap())
            .collect();
        let out_of_bounds = samples.iter().filter(|&&s| s.abs() > 1.0).count();
        let within_bounds_percentage =
            ((samples.len() - out_of_bounds) as f64 / samples.len() as f64) * 100.0;

        assert!(
            within_bounds_percentage > 99.0,
            "Expected >99% of Gaussian samples within [-1.0, 1.0], got {within_bounds_percentage:.1}%"
        );
    }

    #[test]
    fn test_pink_noise_properties() {
        let mut generator = Pink::new(TEST_SAMPLE_RATE);
        let samples: Vec<f32> = (0..TEST_SAMPLES_MEDIUM)
            .map(|_| generator.next().unwrap())
            .collect();

        // Pink noise should have more correlation between consecutive samples than white noise
        let mut correlation_sum = 0.0;
        for i in 0..samples.len() - 1 {
            correlation_sum += samples[i] * samples[i + 1];
        }
        let avg_correlation = correlation_sum / (samples.len() - 1) as f32;

        // Pink noise should have some positive correlation (though not as strong as Brownian)
        assert!(
            avg_correlation > -0.1,
            "Pink noise should have low positive correlation, got: {avg_correlation}"
        );
    }

    #[test]
    fn test_blue_noise_properties() {
        let mut generator = Blue::new(TEST_SAMPLE_RATE);
        let samples: Vec<f32> = (0..TEST_SAMPLES_MEDIUM)
            .map(|_| generator.next().unwrap())
            .collect();

        // Blue noise should have less correlation than pink noise
        let mut correlation_sum = 0.0;
        for i in 0..samples.len() - 1 {
            correlation_sum += samples[i] * samples[i + 1];
        }
        let avg_correlation = correlation_sum / (samples.len() - 1) as f32;

        // Blue noise should have near-zero or negative correlation
        assert!(
            avg_correlation < 0.1,
            "Blue noise should have low correlation, got: {avg_correlation}"
        );
    }

    #[test]
    fn test_violet_noise_properties() {
        let mut generator = Violet::new(TEST_SAMPLE_RATE);
        let samples: Vec<f32> = (0..TEST_SAMPLES_MEDIUM)
            .map(|_| generator.next().unwrap())
            .collect();

        // Violet noise should have high-frequency characteristics
        // Check that consecutive differences have higher variance than the original signal
        let mut diff_variance = 0.0;
        let mut signal_variance = 0.0;
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;

        for i in 0..samples.len() - 1 {
            let diff = samples[i + 1] - samples[i];
            diff_variance += diff * diff;
            let centered = samples[i] - mean;
            signal_variance += centered * centered;
        }

        diff_variance /= (samples.len() - 1) as f32;
        signal_variance /= samples.len() as f32;

        // For violet noise (high-pass), differences should have comparable or higher variance
        assert!(
            diff_variance > signal_variance * 0.1,
            "Violet noise should have high-frequency characteristics, diff_var: {diff_variance}, signal_var: {signal_variance}"
        );
    }

    #[test]
    fn test_brownian_noise_properties() {
        // Test that brownian noise doesn't accumulate DC indefinitely
        let mut generator = Brownian::new(TEST_SAMPLE_RATE);
        let samples: Vec<f32> = (0..TEST_SAMPLE_RATE * 10)
            .map(|_| generator.next().unwrap())
            .collect(); // 10 seconds

        let average = samples.iter().sum::<f32>() / samples.len() as f32;
        // Average should be close to zero due to leak factor
        assert!(
            average.abs() < 0.5,
            "Brownian noise average too far from zero: {average}"
        );

        // Brownian noise should have strong positive correlation between consecutive samples
        let mut correlation_sum = 0.0;
        for i in 0..samples.len() - 1 {
            correlation_sum += samples[i] * samples[i + 1];
        }
        let avg_correlation = correlation_sum / (samples.len() - 1) as f32;

        assert!(
            avg_correlation > 0.1,
            "Brownian noise should have strong positive correlation: {avg_correlation}"
        );
    }

    #[test]
    fn test_velvet_noise_properties() {
        let mut generator = Velvet::new(TEST_SAMPLE_RATE);
        let mut impulse_count = 0;

        for _ in 0..TEST_SAMPLE_RATE {
            let sample = generator.next().unwrap();
            if sample != 0.0 {
                impulse_count += 1;
                // Velvet impulses should be exactly +1.0 or -1.0
                assert!(sample == 1.0 || sample == -1.0);
            }
        }

        assert!(
            impulse_count > (VELVET_DEFAULT_DENSITY * 0.75) as usize
                && impulse_count < (VELVET_DEFAULT_DENSITY * 1.25) as usize,
            "Impulse count out of range: expected ~{VELVET_DEFAULT_DENSITY}, got {impulse_count}"
        );
    }

    #[test]
    fn test_velvet_custom_density() {
        let density = 1000.0; // impulses per second for testing
        let mut generator = Velvet::<SmallRng>::new_with_density(TEST_SAMPLE_RATE, density);

        let mut impulse_count = 0;
        for _ in 0..TEST_SAMPLE_RATE {
            if generator.next().unwrap() != 0.0 {
                impulse_count += 1;
            }
        }

        // Should be approximately the requested density
        let actual_density = impulse_count as f32;
        assert!(
            (actual_density - density).abs() < 200.0,
            "Custom density not achieved: expected ~{density}, got {actual_density}"
        );
    }
}
