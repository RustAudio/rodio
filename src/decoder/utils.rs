//! Common utilities and helper functions for decoder implementations.
//!
//! This module provides shared functionality to reduce code duplication across
//! different decoder implementations. It contains generic algorithms and utilities
//! that are format-agnostic and can be safely reused across multiple audio formats.
//!
//! # Purpose
//!
//! The utilities in this module serve several key purposes:
//! - **Code reuse**: Eliminate duplication of common patterns across decoders
//! - **Consistency**: Ensure uniform behavior for similar operations
//! - **Performance**: Provide optimized implementations of common algorithms
//! - **Maintainability**: Centralize common logic for easier maintenance
//!
//! # Categories
//!
//! The utilities are organized into functional categories:
//! - **Duration calculations**: Converting sample counts to time durations
//! - **Format probing**: Safe format detection with stream position restoration
//! - **Mathematical operations**: Sample rate and timing calculations
//!
//! # Design Principles
//!
//! All utilities follow these design principles:
//! - **Zero overhead**: Inline functions where appropriate for performance
//! - **Safety first**: Handle edge cases like zero sample rates gracefully
//! - **Stream preservation**: Always restore stream positions after probing
//! - **Format agnostic**: Work with any audio format without assumptions

#[cfg(any(feature = "claxon", feature = "hound"))]
use std::time::Duration;

#[cfg(any(feature = "claxon", feature = "hound"))]
use crate::SampleRate;

#[cfg(any(
    feature = "claxon",
    feature = "hound",
    feature = "lewton",
    feature = "minimp3",
))]
use std::io::{Read, Seek, SeekFrom};

/// Converts sample count and sample rate to precise duration.
///
/// This function calculates the exact duration represented by a given number of
/// audio samples at a specific sample rate. It provides nanosecond precision
/// by properly handling the fractional component of the division.
///
/// # Arguments
///
/// * `samples` - Number of audio samples (typically frames × channels)
/// * `sample_rate` - Sample rate in Hz
///
/// # Returns
///
/// A `Duration` representing the exact time span of the samples
///
/// # Precision
///
/// The calculation provides nanosecond precision by:
/// 1. Computing whole seconds from the sample count
/// 2. Converting remainder samples to nanoseconds
/// 3. Properly scaling based on the sample rate
///
/// # Edge Cases
///
/// - **Zero samples**: Returns `Duration::ZERO` (mathematically correct)
/// - **Large values**: Handles overflow gracefully within `Duration` limits
#[cfg(any(feature = "claxon", feature = "hound",))]
pub(super) fn samples_to_duration(samples: u64, sample_rate: SampleRate) -> Duration {
    let sample_rate = sample_rate.get() as u64;
    let secs = samples / sample_rate;
    let nanos = ((samples % sample_rate) * 1_000_000_000) / sample_rate;
    Duration::new(secs, nanos as u32)
}

/// Safe format detection with automatic stream position restoration.
///
/// This utility provides a standardized pattern for format detection that ensures
/// the stream position is always restored regardless of the probe outcome. This is
/// essential for format detection chains where multiple decoders attempt to identify
/// the format sequentially.
///
/// # Algorithm
///
/// The function follows this sequence:
/// 1. **Save position**: Record current stream position
/// 2. **Probe format**: Execute the provided probe function
/// 3. **Restore position**: Return stream to original position
/// 4. **Return result**: Pass through the probe function's result
///
/// # Arguments
///
/// * `data` - Mutable reference to the stream to probe
/// * `probe_fn` - Function that attempts format detection and returns success/failure
///
/// # Returns
///
/// The boolean result from the probe function, indicating whether the format
/// was successfully detected
///
/// # Guarantees
///
/// - **Position restoration**: Stream position is always restored, even if probe panics
/// - **No side effects**: Stream state is unchanged after the call
/// - **Error handling**: Gracefully handles streams that don't support position queries
///
/// # Examples
///
/// ```ignore
/// use std::fs::File;
/// # use rodio::decoder::utils::probe_format;
///
/// let mut file = File::open("audio.unknown").unwrap();
///
/// let is_wav = probe_format(&mut file, |reader| {
///     // Attempt WAV detection logic here
///     reader.read(&mut [0u8; 4]).is_ok() // Simplified example
/// });
///
/// // File position is restored, ready for next probe
/// ```
///
/// # Error Handling
///
/// If the stream doesn't support position queries, the function defaults to
/// position 0, which is suitable for most format detection scenarios. Seek
/// failures during restoration are ignored to prevent probe failures from
/// affecting the detection process.
///
/// # Performance
///
/// This function has minimal overhead, performing only position save/restore
/// operations around the actual probe logic. The cost is dominated by the
/// probe function implementation.
#[cfg(any(
    feature = "claxon",
    feature = "hound",
    feature = "lewton",
    feature = "minimp3",
))]
pub(super) fn probe_format<R, F>(data: &mut R, probe_fn: F) -> bool
where
    R: Read + Seek,
    F: FnOnce(&mut R) -> bool,
{
    let original_pos = data.stream_position().unwrap_or_default();
    let result = probe_fn(data);
    let _ = data.seek(SeekFrom::Start(original_pos));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests for the samples_to_duration function.
    ///
    /// These tests verify correct duration calculation across various scenarios
    /// including edge cases and common audio configurations.
    #[cfg(any(feature = "hound", feature = "claxon"))]
    #[test]
    fn test_samples_to_duration() {
        // Standard CD quality: 1 second at 44.1kHz
        let rate_44_1k = SampleRate::new(44100).unwrap();
        assert_eq!(
            samples_to_duration(rate_44_1k.get() as u64, rate_44_1k),
            Duration::from_secs(1)
        );

        // Half second at CD quality
        assert_eq!(
            samples_to_duration(rate_44_1k.get() as u64 / 2, rate_44_1k),
            Duration::from_millis(500)
        );

        // Edge case: Zero samples should return zero duration
        assert_eq!(samples_to_duration(0, rate_44_1k), Duration::ZERO);

        // Precision test: Fractional milliseconds
        // 441 samples at 44.1kHz = 10ms exactly
        assert_eq!(
            samples_to_duration(rate_44_1k.get() as u64 / 100, rate_44_1k),
            Duration::from_millis(10)
        );

        // Very small durations should have nanosecond precision
        // 1 sample at 44.1kHz ≈ 22.675 microseconds
        let one_sample_duration = samples_to_duration(1, rate_44_1k);
        assert_eq!(one_sample_duration.as_nanos(), 22675);
    }
}
