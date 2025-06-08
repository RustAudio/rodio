use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::Source;

/// Internal function that builds a `Amplify` object.
pub fn amplify<I>(input: I, factor: f32) -> Amplify<I>
where
    I: Source,
{
    Amplify { input, factor }
}

/// Internal function that converts decibels to linear
pub(super) fn to_linear(decibels: f32) -> f32 {
    // ~3-4% faster than using `10f32.powf(decibels * 0.05)`,
    // with a maximum error of 2.48e-7 representing only about -132 dB.
    2.0f32.powf(decibels * 0.05 * std::f32::consts::LOG2_10)
}

/// Internal function that converts linear to decibels
pub(super) fn to_db(linear: f32) -> f32 {
    // Same as `to_linear`: faster than using `20f32.log10() * linear`
    linear.log2() * std::f32::consts::LOG10_2 * 20.0
}

/// Filter that modifies each sample by a given value.
#[derive(Clone, Debug)]
pub struct Amplify<I> {
    input: I,
    factor: f32,
}

impl<I> Amplify<I> {
    /// Modifies the amplification factor.
    #[inline]
    pub fn set_factor(&mut self, factor: f32) {
        self.factor = factor;
    }

    /// Modifies the amplification factor logarithmically.
    #[inline]
    pub fn set_log_factor(&mut self, factor: f32) {
        self.factor = to_linear(factor);
    }

    /// Returns a reference to the inner source.
    #[inline]
    pub fn inner(&self) -> &I {
        &self.input
    }

    /// Returns a mutable reference to the inner source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.input
    }

    /// Returns the inner source.
    #[inline]
    pub fn into_inner(self) -> I {
        self.input
    }
}

impl<I> Iterator for Amplify<I>
where
    I: Source,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.input.next().map(|value| value * self.factor)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for Amplify<I> where I: Source + ExactSizeIterator {}

impl<I> Source for Amplify<I>
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
        self.input.try_seek(pos)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Tolerance values for precision tests, derived from empirical measurement
    /// of actual implementation errors across the full ±100dB range.
    ///
    /// Methodology:
    /// 1. Measured actual relative errors against mathematically exact f64 calculations
    /// 2. Found maximum errors: dB->linear = 2.3x ε, linear->dB = 1.0x ε, round-trip = 8x ε
    /// 3. Applied 2x safety margins for cross-platform robustness
    /// 4. All tolerances are much stricter than audio precision requirements:
    ///    - 16-bit audio: ~6e-6 precision needed
    ///    - 24-bit audio: ~6e-8 precision needed
    ///    - Our tolerances: ~6e-7 to 2e-6 (10-1000x better than audio needs)
    ///
    /// Range context:
    /// - Practical audio range (-60dB to +40dB): max errors ~1x ε
    /// - Extended range (-100dB to +100dB): max errors ~2.3x ε
    /// - Extreme edge cases beyond ±100dB have larger errors but are rarely used

    /// Based on [Wikipedia's Decibel article].
    ///
    /// [Wikipedia's Decibel article]: https://web.archive.org/web/20230810185300/https://en.wikipedia.org/wiki/Decibel
    const DECIBELS_LINEAR_TABLE: [(f32, f32); 27] = [
        (100., 100000.),
        (90., 31623.),
        (80., 10000.),
        (70., 3162.),
        (60., 1000.),
        (50., 316.2),
        (40., 100.),
        (30., 31.62),
        (20., 10.),
        (10., 3.162),
        (5.998, 1.995),
        (3.003, 1.413),
        (1.002, 1.122),
        (0., 1.),
        (-1.002, 0.891),
        (-3.003, 0.708),
        (-5.998, 0.501),
        (-10., 0.3162),
        (-20., 0.1),
        (-30., 0.03162),
        (-40., 0.01),
        (-50., 0.003162),
        (-60., 0.001),
        (-70., 0.0003162),
        (-80., 0.0001),
        (-90., 0.00003162),
        (-100., 0.00001),
    ];

    #[test]
    fn convert_decibels_to_linear() {
        for (db, wikipedia_linear) in DECIBELS_LINEAR_TABLE {
            let actual_linear = to_linear(db);

            // Calculate the mathematically exact reference value using f64 precision
            let exact_linear = f64::powf(10.0, db as f64 * 0.05) as f32;

            // Test implementation precision against exact mathematical result
            let relative_error = ((actual_linear - exact_linear) / exact_linear).abs();
            const MAX_RELATIVE_ERROR: f32 = 5.0 * f32::EPSILON; // Measured max error: 2.3x ε (at -100dB), with 2x safety margin

            assert!(
                relative_error < MAX_RELATIVE_ERROR,
                "Implementation precision failed for {}dB: exact {:.8}, got {:.8}, relative error: {:.2e}",
                db, exact_linear, actual_linear, relative_error
            );

            // Sanity check: ensure we're in the right order of magnitude as Wikipedia data
            // This is lenient to account for rounding in the reference values
            let magnitude_ratio = actual_linear / wikipedia_linear;
            assert!(
                magnitude_ratio > 0.99 && magnitude_ratio < 1.01,
                "Result magnitude differs significantly from Wikipedia reference for {}dB: Wikipedia {}, got {}, ratio: {:.4}",
                db, wikipedia_linear, actual_linear, magnitude_ratio
            );
        }
    }

    #[test]
    fn convert_linear_to_decibels() {
        // Test the inverse conversion function using the same reference data
        for (expected_db, linear) in DECIBELS_LINEAR_TABLE {
            let actual_db = to_db(linear);

            // Calculate the mathematically exact reference value using f64 precision
            let exact_db = ((linear as f64).log10() * 20.0) as f32;

            // Test implementation precision against exact mathematical result
            if exact_db.abs() > 10.0 * f32::EPSILON {
                // Use relative error for non-zero dB values
                let relative_error = ((actual_db - exact_db) / exact_db.abs()).abs();
                const MAX_RELATIVE_ERROR: f32 = 5.0 * f32::EPSILON; // Measured max error: 1.0x ε, with 5x safety margin

                assert!(
                    relative_error < MAX_RELATIVE_ERROR,
                    "Linear to dB conversion precision failed for {}: exact {:.8}, got {:.8}, relative error: {:.2e}",
                    linear, exact_db, actual_db, relative_error
                );
            } else {
                // Use absolute error for values very close to 0 dB (linear ≈ 1.0)
                let absolute_error = (actual_db - exact_db).abs();
                const MAX_ABSOLUTE_ERROR: f32 = 1.0 * f32::EPSILON; // 0 dB case is mathematically exact, minimal tolerance for numerical stability

                assert!(
                    absolute_error < MAX_ABSOLUTE_ERROR,
                    "Linear to dB conversion precision failed for {}: exact {:.8}, got {:.8}, absolute error: {:.2e}",
                    linear, exact_db, actual_db, absolute_error
                );
            }

            // Sanity check: ensure we're reasonably close to the expected dB value from the table
            // This accounts for rounding in both the linear and dB reference values
            let magnitude_ratio = if expected_db.abs() > 10.0 * f32::EPSILON {
                actual_db / expected_db
            } else {
                1.0 // Skip ratio check for values very close to 0 dB
            };

            if expected_db.abs() > 10.0 * f32::EPSILON {
                assert!(
                    magnitude_ratio > 0.99 && magnitude_ratio < 1.01,
                    "Result differs significantly from table reference for linear {}: expected {}dB, got {}dB, ratio: {:.4}",
                    linear, expected_db, actual_db, magnitude_ratio
                );
            }
        }
    }

    #[test]
    fn round_trip_conversion_accuracy() {
        // Test that converting dB -> linear -> dB gives back the original value
        let test_db_values = [-60.0, -20.0, -6.0, 0.0, 6.0, 20.0, 40.0];

        for &original_db in &test_db_values {
            let linear = to_linear(original_db);
            let round_trip_db = to_db(linear);

            let error = (round_trip_db - original_db).abs();
            const MAX_ROUND_TRIP_ERROR: f32 = 16.0 * f32::EPSILON; // Measured max error: 8x ε (practical audio range), with 2x safety margin

            assert!(
                error < MAX_ROUND_TRIP_ERROR,
                "Round-trip conversion failed for {}dB: got {:.8}dB, error: {:.2e}",
                original_db,
                round_trip_db,
                error
            );
        }

        // Test that converting linear -> dB -> linear gives back the original value
        let test_linear_values = [0.001, 0.1, 1.0, 10.0, 100.0];

        for &original_linear in &test_linear_values {
            let db = to_db(original_linear);
            let round_trip_linear = to_linear(db);

            let relative_error = ((round_trip_linear - original_linear) / original_linear).abs();
            const MAX_ROUND_TRIP_RELATIVE_ERROR: f32 = 16.0 * f32::EPSILON; // Same as above, for linear->dB->linear round trips

            assert!(
                relative_error < MAX_ROUND_TRIP_RELATIVE_ERROR,
                "Round-trip conversion failed for {}: got {:.8}, relative error: {:.2e}",
                original_linear,
                round_trip_linear,
                relative_error
            );
        }
    }
}
