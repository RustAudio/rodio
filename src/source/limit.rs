//! Audio peak limiting for dynamic range control.
//!
//! This module implements a feedforward limiter that prevents audio peaks from exceeding
//! a specified threshold while maintaining audio quality. The limiter is based on:
//! Giannoulis, D., Massberg, M., & Reiss, J.D. (2012). Digital Dynamic Range Compressor Design,
//! A Tutorial and Analysis. Journal of The Audio Engineering Society, 60, 399-408.
//!
//! # What is Limiting?
//!
//! A limiter reduces the amplitude of audio signals that exceed a threshold level.
//! For example, with a -6dB threshold, peaks above that level are reduced
//! to stay near the threshold, preventing clipping and maintaining consistent output levels.
//!
//! # Features
//!
//! * **Soft-knee limiting** - Gradual transition into limiting for natural sound
//! * **Per-channel detection** - Decoupled peak detection per channel
//! * **Coupled gain reduction** - Uniform gain reduction across channels preserves stereo imaging
//! * **Configurable timing** - Adjustable attack/release times for different use cases
//! * **Efficient processing** - Optimized implementations for mono, stereo, and multi-channel audio
//!
//! # Usage
//!
//! Use [`LimitSettings`] to configure the limiter, then apply it to any audio source:
//!
//! ```rust
//! use rodio::source::{SineWave, Source, LimitSettings};
//! use std::time::Duration;
//!
//! // Create a loud sine wave
//! let source = SineWave::new(440.0).amplify(2.0);
//!
//! // Apply limiting with -6dB threshold
//! let settings = LimitSettings::default().with_threshold(-6.0);
//! let limited = source.limit(settings);
//! ```

use std::time::Duration;

use super::SeekError;
use crate::{
    common::{ChannelCount, Sample, SampleRate},
    source::amplify,
    Source,
};

/// Configuration settings for audio limiting.
///
/// This struct defines how the limiter behaves, including when to start limiting
/// (threshold), how gradually to apply it (knee width), and how quickly to respond
/// to level changes (attack/release times).
///
/// # Parameters
///
/// * **Threshold** - Level in dB where limiting begins (must be negative, typically -1 to -6 dB)
/// * **Knee Width** - Range in dB over which limiting gradually increases (wider = smoother)
/// * **Attack** - Time to respond to level increases (shorter = faster but may distort)
/// * **Release** - Time to recover after level decreases (longer = smoother)
///
/// # Examples
///
/// ## Basic Usage
///
/// ```rust
/// use rodio::source::{SineWave, Source, LimitSettings};
/// use std::time::Duration;
///
/// // Use default settings (-1 dB threshold, 4 dB knee, 5ms attack, 100ms release)
/// let source = SineWave::new(440.0).amplify(2.0);
/// let limited = source.limit(LimitSettings::default());
/// ```
///
/// ## Custom Settings with Builder Pattern
///
/// ```rust
/// use rodio::source::{SineWave, Source, LimitSettings};
/// use std::time::Duration;
///
/// let source = SineWave::new(440.0).amplify(3.0);
/// let settings = LimitSettings::new()
///     .with_threshold(-6.0)                    // Limit peaks above -6dB
///     .with_knee_width(2.0)                    // 2dB soft knee for smooth limiting
///     .with_attack(Duration::from_millis(3))   // Fast 3ms attack
///     .with_release(Duration::from_millis(50)); // 50ms release
///
/// let limited = source.limit(settings);
/// ```
///
/// ## Common Adjustments
///
/// ```rust
/// use rodio::source::LimitSettings;
/// use std::time::Duration;
///
/// // More headroom for dynamic content
/// let conservative = LimitSettings::default()
///     .with_threshold(-3.0)                    // More headroom
///     .with_knee_width(6.0);                   // Wide knee for transparency
///
/// // Tighter control for broadcast/streaming
/// let broadcast = LimitSettings::default()
///     .with_knee_width(2.0)                    // Narrower knee for firmer limiting
///     .with_attack(Duration::from_millis(3))   // Faster attack
///     .with_release(Duration::from_millis(50)); // Faster release
/// ```
#[derive(Debug, Clone)]
pub struct LimitSettings {
    /// Level where limiting begins (dB, must be negative)
    pub threshold: f32,
    /// Range over which limiting gradually increases (dB)
    pub knee_width: f32,
    /// Time to respond to level increases
    pub attack: Duration,
    /// Time to recover after level decreases
    pub release: Duration,
}

impl Default for LimitSettings {
    fn default() -> Self {
        Self {
            threshold: -1.0,                     // -1 dB
            knee_width: 4.0,                     // 4 dB
            attack: Duration::from_millis(5),    // 5 ms
            release: Duration::from_millis(100), // 100 ms
        }
    }
}

impl LimitSettings {
    /// Creates new limit settings with default values.
    ///
    /// Equivalent to [`LimitSettings::default()`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the threshold level where limiting begins.
    ///
    /// # Arguments
    ///
    /// * `threshold` - Level in dB where limiting starts (must be negative)
    ///   - `-1.0` = limiting starts 1dB below 0dBFS (tight limiting, prevents clipping)
    ///   - `-3.0` = limiting starts 3dB below 0dBFS (balanced approach)
    ///   - `-6.0` = limiting starts 6dB below 0dBFS (gentle limiting, preserves dynamics)
    ///
    /// Note: Only negative values make sense - positive values would attempt limiting
    /// above 0dBFS, which cannot prevent clipping and may cause distortion.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    /// Sets the knee width - range over which limiting gradually increases.
    ///
    /// # Arguments
    ///
    /// * `knee_width` - Range in dB over which limiting transitions from off to full
    ///   - Smaller values (0.5-2.0 dB) = harder, more obvious limiting
    ///   - Larger values (4.0-8.0 dB) = softer, more transparent limiting
    pub fn with_knee_width(mut self, knee_width: f32) -> Self {
        self.knee_width = knee_width;
        self
    }

    /// Sets the attack time - how quickly the limiter responds to level increases.
    ///
    /// # Arguments
    ///
    /// * `attack` - Time duration for the limiter to react to peaks
    ///   - Shorter (1-5 ms) = faster response, may cause distortion
    ///   - Longer (10-20 ms) = smoother sound, may allow brief overshoots
    pub fn with_attack(mut self, attack: Duration) -> Self {
        self.attack = attack;
        self
    }

    /// Sets the release time - how quickly the limiter recovers after level decreases.
    ///
    /// # Arguments
    ///
    /// * `release` - Time duration for the limiter to stop limiting
    ///   - Shorter (10-50 ms) = quick recovery, may sound pumping
    ///   - Longer (100-500 ms) = smooth recovery, more natural sound
    pub fn with_release(mut self, release: Duration) -> Self {
        self.release = release;
        self
    }
}

/// Creates a limiter that processes the input audio source.
///
/// This function applies the specified limiting settings to control audio peaks.
/// The limiter uses feedforward processing with configurable attack/release times
/// and soft-knee characteristics for natural-sounding dynamic range control.
///
/// # Arguments
///
/// * `input` - Audio source to process
/// * `settings` - Limiter configuration (threshold, knee, timing)
///
/// # Returns
///
/// A [`Limit`] source that applies the limiting to the input audio.
///
/// # Example
///
/// ```rust
/// use rodio::source::{SineWave, Source, LimitSettings};
///
/// let source = SineWave::new(440.0).amplify(2.0);
/// let settings = LimitSettings::default().with_threshold(-6.0);
/// let limited = source.limit(settings);
/// ```
pub(crate) fn limit<I: Source>(input: I, settings: LimitSettings) -> Limit<I> {
    let sample_rate = input.sample_rate();
    let attack = duration_to_coefficient(settings.attack, sample_rate);
    let release = duration_to_coefficient(settings.release, sample_rate);
    let channels = input.channels() as usize;

    let base = LimitBase::new(settings.threshold, settings.knee_width, attack, release);

    match channels {
        1 => Limit::Mono(LimitMono {
            input,
            base,
            normalisation_integrator: 0.0,
            normalisation_peak: 0.0,
        }),
        2 => Limit::Stereo(LimitStereo {
            input,
            base,
            normalisation_integrators: [0.0; 2],
            normalisation_peaks: [0.0; 2],
            position: 0,
        }),
        n => Limit::MultiChannel(LimitMulti {
            input,
            base,
            normalisation_integrators: vec![0.0; n],
            normalisation_peaks: vec![0.0; n],
            position: 0,
        }),
    }
}

/// A source filter that applies audio limiting to prevent peaks from exceeding a threshold.
///
/// This filter reduces the amplitude of audio signals that exceed the configured threshold
/// level, helping to prevent clipping and maintain consistent output levels. The limiter
/// automatically adapts to mono, stereo, or multi-channel audio sources.
///
/// # How it Works
///
/// The limiter detects peaks in each audio channel independently but applies gain reduction
/// uniformly across all channels. This preserves stereo imaging while ensuring that loud
/// peaks in any channel are controlled.
///
/// # Created By
///
/// Use [`Source::limit()`] with [`LimitSettings`] to create a `Limit` source:
///
/// ```rust
/// use rodio::source::{SineWave, Source, LimitSettings};
///
/// let source = SineWave::new(440.0).amplify(2.0);
/// let limited = source.limit(LimitSettings::default().with_threshold(-6.0));
/// ```
///
/// # Type Parameters
///
/// * `I` - The input audio source type
#[derive(Clone, Debug)]
pub enum Limit<I>
where
    I: Source,
{
    /// Mono channel limiter
    Mono(LimitMono<I>),
    /// Stereo channel limiter
    Stereo(LimitStereo<I>),
    /// Multi-channel limiter for arbitrary channel counts
    MultiChannel(LimitMulti<I>),
}

/// Common parameters and processing logic shared across all limiter variants.
///
/// Handles:
/// * Parameter storage (threshold, knee width, attack/release coefficients)
/// * Per-channel state updates for peak detection
/// * Gain computation through soft-knee limiting
#[derive(Clone, Debug)]
struct LimitBase {
    /// Level where limiting begins (dB)
    threshold: f32,
    /// Width of the soft-knee region (dB)
    knee_width: f32,
    /// Inverse of 8 times the knee width (precomputed for efficiency)
    inv_knee_8: f32,
    /// Attack time constant (ms)
    attack: f32,
    /// Release time constant (ms)
    release: f32,
}

/// Mono channel limiter optimized for single-channel processing
#[derive(Clone, Debug)]
pub struct LimitMono<I> {
    /// Input audio source
    input: I,
    /// Common limiter parameters
    base: LimitBase,
    /// Peak detection integrator state
    normalisation_integrator: f32,
    /// Peak detection state
    normalisation_peak: f32,
}

/// Stereo channel limiter with optimized two-channel processing
#[derive(Clone, Debug)]
pub struct LimitStereo<I> {
    /// Input audio source
    input: I,
    /// Common limiter parameters
    base: LimitBase,
    /// Normalisation integrator states
    normalisation_integrators: [f32; 2],
    /// Normalisation peak states
    normalisation_peaks: [f32; 2],
    /// Current channel position
    position: u8,
}

/// Generic multi-channel normalizer for surround sound or other configurations
#[derive(Clone, Debug)]
pub struct LimitMulti<I> {
    /// Input audio source
    input: I,
    /// Common limiter parameters
    base: LimitBase,
    /// Normalisation integrator states
    normalisation_integrators: Vec<f32>,
    /// Normalisation peak states
    normalisation_peaks: Vec<f32>,
    /// Current channel position
    position: usize,
}

/// Computes the gain reduction amount in dB based on input level.
///
/// Implements soft-knee compression with three regions:
/// 1. Below threshold - knee_width: No compression (returns 0.0)
/// 2. Within knee region: Gradual compression with quadratic curve
/// 3. Above threshold + knee_width: Linear compression
///
/// Optimized for the most common case where samples are below threshold and no limiting is needed
/// (returns `0.0` early).
///
/// # Arguments
///
/// * `sample` - Input sample value (with initial gain applied)
/// * `threshold` - Level where limiting begins (dB)
/// * `knee_width` - Width of soft knee region (dB)
/// * `inv_knee_8` - Precomputed value: 1.0 / (8.0 * knee_width) for efficiency
///
/// # Returns
///
/// Amount of gain reduction to apply in dB
#[inline]
fn process_sample(sample: Sample, threshold: f32, knee_width: f32, inv_knee_8: f32) -> f32 {
    // Add slight DC offset. Some samples are silence, which is -inf dB and gets the limiter stuck.
    // Adding a small positive offset prevents this.
    let bias_db = amplify::to_db(sample.abs() + f32::MIN_POSITIVE) - threshold;
    let knee_boundary_db = bias_db * 2.0;
    if knee_boundary_db < -knee_width {
        0.0
    } else if knee_boundary_db.abs() <= knee_width {
        // Faster than powi(2)
        let x = knee_boundary_db + knee_width;
        x * x * inv_knee_8
    } else {
        bias_db
    }
}

impl LimitBase {
    fn new(threshold: f32, knee_width: f32, attack: f32, release: f32) -> Self {
        let inv_knee_8 = 1.0 / (8.0 * knee_width);
        Self {
            threshold,
            knee_width,
            inv_knee_8,
            attack,
            release,
        }
    }

    /// Updates the channel's envelope detection state.
    ///
    /// For each channel, processes:
    /// 1. Initial gain and dB conversion
    /// 2. Soft-knee limiting calculation
    /// 3. Envelope detection with attack/release filtering
    /// 4. Peak level tracking
    ///
    /// The envelope detection uses a dual-stage approach:
    /// - First stage: Max of current signal and smoothed release
    /// - Second stage: Attack smoothing of the peak detector output
    ///
    /// Note: Only updates state, gain application is handled by the variant implementations to
    /// allow for coupled gain reduction across channels.
    #[must_use]
    #[inline]
    fn process_channel(&self, sample: Sample, integrator: &mut f32, peak: &mut f32) -> Sample {
        // step 1-4: half-wave rectification and conversion into dB, and gain computer with soft
        // knee and subtractor
        let limiter_db = process_sample(sample, self.threshold, self.knee_width, self.inv_knee_8);

        // step 5: smooth, decoupled peak detector
        *integrator = f32::max(
            limiter_db,
            self.release * *integrator + (1.0 - self.release) * limiter_db,
        );
        *peak = self.attack * *peak + (1.0 - self.attack) * *integrator;

        sample
    }
}

impl<I> LimitMono<I>
where
    I: Source,
{
    /// Processes the next mono sample through the limiter.
    ///
    /// Single channel implementation with direct state updates.
    #[inline]
    fn process_next(&mut self, sample: I::Item) -> I::Item {
        let processed = self.base.process_channel(
            sample,
            &mut self.normalisation_integrator,
            &mut self.normalisation_peak,
        );

        // steps 6-8: conversion into level and multiplication into gain stage
        processed * amplify::to_linear(-self.normalisation_peak)
    }
}

impl<I> LimitStereo<I>
where
    I: Source,
{
    /// Processes the next stereo sample through the limiter.
    ///
    /// Uses efficient channel position tracking with XOR toggle and direct array access for state
    /// updates.
    #[inline]
    fn process_next(&mut self, sample: I::Item) -> I::Item {
        let channel = self.position as usize;
        self.position ^= 1;

        let processed = self.base.process_channel(
            sample,
            &mut self.normalisation_integrators[channel],
            &mut self.normalisation_peaks[channel],
        );

        // steps 6-8: conversion into level and multiplication into gain stage. Find maximum peak
        // across both channels to couple the gain and maintain stereo imaging.
        let max_peak = f32::max(self.normalisation_peaks[0], self.normalisation_peaks[1]);
        processed * amplify::to_linear(-max_peak)
    }
}

impl<I> LimitMulti<I>
where
    I: Source,
{
    /// Processes the next multi-channel sample through the limiter.
    ///
    /// Generic implementation supporting arbitrary channel counts with `Vec`-based state storage.
    #[inline]
    fn process_next(&mut self, sample: I::Item) -> I::Item {
        let channel = self.position;
        self.position = (self.position + 1) % self.normalisation_integrators.len();

        let processed = self.base.process_channel(
            sample,
            &mut self.normalisation_integrators[channel],
            &mut self.normalisation_peaks[channel],
        );

        // steps 6-8: conversion into level and multiplication into gain stage. Find maximum peak
        // across all channels to couple the gain and maintain multi-channel imaging.
        let max_peak = self
            .normalisation_peaks
            .iter()
            .fold(0.0, |max, &peak| f32::max(max, peak));
        processed * amplify::to_linear(-max_peak)
    }
}

impl<I> Limit<I>
where
    I: Source,
{
    /// Returns a reference to the inner audio source.
    ///
    /// Routes through the enum variant to access the underlying source, preserving the specialized
    /// implementation structure while allowing source inspection.
    ///
    /// Useful for inspecting source properties without consuming the filter.
    #[inline]
    pub fn inner(&self) -> &I {
        match self {
            Limit::Mono(mono) => &mono.input,
            Limit::Stereo(stereo) => &stereo.input,
            Limit::MultiChannel(multi) => &multi.input,
        }
    }

    /// Returns a mutable reference to the inner audio source.
    ///
    /// Routes through the enum variant to access the underlying source, maintaining the
    /// specialized implementation structure while allowing source modification.
    ///
    /// Essential for operations like seeking that need to modify the source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        match self {
            Limit::Mono(mono) => &mut mono.input,
            Limit::Stereo(stereo) => &mut stereo.input,
            Limit::MultiChannel(multi) => &mut multi.input,
        }
    }

    /// Consumes the filter and returns the inner audio source.
    ///
    /// Dismantles the normalizer variant to extract the source, allowing the audio pipeline to
    /// continue without normalization overhead.
    ///
    /// Useful when normalization is no longer needed but source should continue.
    #[inline]
    pub fn into_inner(self) -> I {
        match self {
            Limit::Mono(mono) => mono.input,
            Limit::Stereo(stereo) => stereo.input,
            Limit::MultiChannel(multi) => multi.input,
        }
    }
}

impl<I> Iterator for Limit<I>
where
    I: Source,
{
    type Item = I::Item;

    /// Provides the next processed sample.
    ///
    /// Routes processing to the appropriate channel-specific implementation:
    /// * Mono: Direct single-channel processing
    /// * Stereo: Optimized two-channel processing
    /// * `MultiChannel`: Generic multi-channel processing
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Limit::Mono(mono) => {
                let sample = mono.input.next()?;
                Some(mono.process_next(sample))
            }
            Limit::Stereo(stereo) => {
                let sample = stereo.input.next()?;
                Some(stereo.process_next(sample))
            }
            Limit::MultiChannel(multi) => {
                let sample = multi.input.next()?;
                Some(multi.process_next(sample))
            }
        }
    }

    /// Provides size hints from the inner source.
    ///
    /// Delegates directly to the source to maintain accurate collection sizing.
    /// Used by collection operations for optimization.
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner().size_hint()
    }
}

impl<I> Source for Limit<I>
where
    I: Source,
{
    /// Returns the number of samples in the current audio frame.
    ///
    /// Delegates to inner source to maintain frame alignment.
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.inner().current_span_len()
    }

    /// Returns the number of channels in the audio stream.
    ///
    /// Channel count determines which normalizer variant is used:
    /// * 1: Mono
    /// * 2: Stereo
    /// * >2: MultiChannel
    fn channels(&self) -> ChannelCount {
        self.inner().channels()
    }

    /// Returns the audio sample rate in Hz.
    fn sample_rate(&self) -> SampleRate {
        self.inner().sample_rate()
    }

    /// Returns the total duration of the audio.
    ///
    /// Returns None for streams without known duration.
    fn total_duration(&self) -> Option<Duration> {
        self.inner().total_duration()
    }

    /// Attempts to seek to the specified position.
    ///
    /// Resets limiter state to prevent artifacts after seeking:
    /// * Mono: Direct reset of integrator and peak values
    /// * Stereo: Efficient array fill for both channels
    /// * `MultiChannel`: Resets all channel states via fill
    ///
    /// # Arguments
    ///
    /// * `target` - Position to seek to
    ///
    /// # Errors
    ///
    /// Returns error if the underlying source fails to seek
    fn try_seek(&mut self, target: Duration) -> Result<(), SeekError> {
        self.inner_mut().try_seek(target)?;

        match self {
            Limit::Mono(mono) => {
                mono.normalisation_integrator = 0.0;
                mono.normalisation_peak = 0.0;
            }
            Limit::Stereo(stereo) => {
                stereo.normalisation_integrators.fill(0.0);
                stereo.normalisation_peaks.fill(0.0);
            }
            Limit::MultiChannel(multi) => {
                multi.normalisation_integrators.fill(0.0);
                multi.normalisation_peaks.fill(0.0);
            }
        }

        Ok(())
    }
}

/// Converts a time duration to a smoothing coefficient for exponential filtering.
///
/// Used for both attack and release filtering in the limiter's envelope detector.
/// Creates a coefficient that determines how quickly the limiter responds to level changes:
/// * Longer times = higher coefficients (closer to 1.0) = slower, smoother response
/// * Shorter times = lower coefficients (closer to 0.0) = faster, more immediate response
///
/// The coefficient is calculated using the formula: `e^(-1 / (duration_seconds * sample_rate))`
/// which provides exponential smoothing behavior suitable for audio envelope detection.
///
/// # Arguments
///
/// * `duration` - Desired response time (attack or release duration)
/// * `sample_rate` - Audio sample rate in Hz
///
/// # Returns
///
/// Smoothing coefficient in the range [0.0, 1.0] for use in exponential filters
#[must_use]
fn duration_to_coefficient(duration: Duration, sample_rate: SampleRate) -> f32 {
    f32::exp(-1.0 / (duration.as_secs_f32() * sample_rate as f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::SamplesBuffer;
    use crate::source::{SineWave, Source};
    use std::time::Duration;

    fn create_test_buffer(samples: Vec<f32>, channels: u16, sample_rate: u32) -> SamplesBuffer {
        SamplesBuffer::new(channels, sample_rate, samples)
    }

    #[test]
    fn test_limiter_creation() {
        // Test mono
        let buffer = create_test_buffer(vec![0.5, 0.8, 1.0, 0.3], 1, 44100);
        let limiter = limit(buffer, LimitSettings::default());
        assert_eq!(limiter.channels(), 1);
        assert_eq!(limiter.sample_rate(), 44100);
        matches!(limiter, Limit::Mono(_));

        // Test stereo
        let buffer = create_test_buffer(vec![0.5, 0.8, 1.0, 0.3, 0.2, 0.6, 0.9, 0.4], 2, 44100);
        let limiter = limit(buffer, LimitSettings::default());
        assert_eq!(limiter.channels(), 2);
        matches!(limiter, Limit::Stereo(_));

        // Test multichannel
        let buffer = create_test_buffer(vec![0.5; 12], 3, 44100);
        let limiter = limit(buffer, LimitSettings::default());
        assert_eq!(limiter.channels(), 3);
        matches!(limiter, Limit::MultiChannel(_));
    }

    #[test]
    fn test_limiting_works() {
        // High amplitude sine wave limited to -6dB
        let sine_wave = SineWave::new(440.0)
            .amplify(3.0) // 3.0 linear = ~9.5dB
            .take_duration(Duration::from_millis(60)); // ~2600 samples

        let settings = LimitSettings::default()
            .with_threshold(-6.0)   // -6dB = ~0.5 linear
            .with_knee_width(0.5)
            .with_attack(Duration::from_millis(3))
            .with_release(Duration::from_millis(12));

        let limiter = sine_wave.limit(settings);
        let samples: Vec<f32> = limiter.take(2600).collect();

        // After settling, ALL samples should be well below 1.0 (around 0.5)
        let settled_samples = &samples[1500..]; // After attack/release settling
        let settled_peak = settled_samples
            .iter()
            .fold(0.0f32, |acc, &x| acc.max(x.abs()));

        assert!(
            settled_peak <= 0.6,
            "Settled peak should be ~0.5 for -6dB: {:.3}",
            settled_peak
        );
        assert!(
            settled_peak >= 0.4,
            "Peak should be reasonably close to 0.5: {:.3}",
            settled_peak
        );

        let max_sample = settled_samples
            .iter()
            .fold(0.0f32, |acc, &x| acc.max(x.abs()));
        assert!(
            max_sample < 0.8,
            "ALL samples should be well below 1.0: max={:.3}",
            max_sample
        );
    }

    #[test]
    fn test_settings_api() {
        let default_settings = LimitSettings::default();
        assert_eq!(default_settings.threshold, -1.0);
        assert_eq!(default_settings.knee_width, 4.0);
        assert_eq!(default_settings.attack, Duration::from_millis(5));
        assert_eq!(default_settings.release, Duration::from_millis(100));

        let custom_settings = LimitSettings::new()
            .with_threshold(-3.0)
            .with_knee_width(2.0)
            .with_attack(Duration::from_millis(10))
            .with_release(Duration::from_millis(50));

        assert_eq!(custom_settings.threshold, -3.0);
        assert_eq!(custom_settings.knee_width, 2.0);
        assert_eq!(custom_settings.attack, Duration::from_millis(10));
        assert_eq!(custom_settings.release, Duration::from_millis(50));
    }

    #[test]
    fn test_passthrough_below_threshold() {
        // Low amplitude signal should pass through unchanged
        let sine_wave = SineWave::new(1000.0)
            .amplify(0.2) // 0.2 linear, well below -6dB threshold
            .take_duration(Duration::from_millis(20));

        let settings = LimitSettings::default().with_threshold(-6.0);

        let original_samples: Vec<f32> = sine_wave.clone().take(880).collect();
        let limiter = sine_wave.limit(settings);
        let limited_samples: Vec<f32> = limiter.take(880).collect();

        // Samples should be nearly identical since below threshold
        for (orig, limited) in original_samples.iter().zip(limited_samples.iter()) {
            let diff = (orig - limited).abs();
            assert!(
                diff < 0.01,
                "Below threshold should pass through: diff={:.6}",
                diff
            );
        }
    }
}
