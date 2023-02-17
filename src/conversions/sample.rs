use cpal::{FromSample, Sample as CpalSample};
use std::marker::PhantomData;

/// Converts the samples data type to `O`.
#[derive(Clone, Debug)]
pub struct DataConverter<I, O> {
    input: I,
    marker: PhantomData<O>,
}

impl<I, O> DataConverter<I, O> {
    /// Builds a new converter.
    #[inline]
    pub fn new(input: I) -> DataConverter<I, O> {
        DataConverter {
            input,
            marker: PhantomData,
        }
    }

    /// Destroys this iterator and returns the underlying iterator.
    #[inline]
    pub fn into_inner(self) -> I {
        self.input
    }
}

impl<I, O> Iterator for DataConverter<I, O>
where
    I: Iterator,
    I::Item: Sample,
    O: FromSample<I::Item> + Sample,
{
    type Item = O;

    #[inline]
    fn next(&mut self) -> Option<O> {
        self.input.next().map(|s| CpalSample::from_sample(s))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I, O> ExactSizeIterator for DataConverter<I, O>
where
    I: ExactSizeIterator,
    I::Item: Sample,
    O: FromSample<I::Item> + Sample,
{
}

/// Represents a value of a single sample.
///
/// This trait is implemented by default on three types: `i16`, `u16` and `f32`.
///
/// - For `i16`, silence corresponds to the value `0`. The minimum and maximum amplitudes are
///   represented by `i16::min_value()` and `i16::max_value()` respectively.
/// - For `u16`, silence corresponds to the value `u16::max_value() / 2`. The minimum and maximum
///   amplitudes are represented by `0` and `u16::max_value()` respectively.
/// - For `f32`, silence corresponds to the value `0.0`. The minimum and maximum amplitudes are
///  represented by `-1.0` and `1.0` respectively.
///
/// You can implement this trait on your own type as well if you wish so.
///
pub trait Sample: CpalSample {
    /// Linear interpolation between two samples.
    ///
    /// The result should be equal to
    /// `first * numerator / denominator + second * (1 - numerator / denominator)`.
    fn lerp(first: Self, second: Self, numerator: u32, denominator: u32) -> Self;
    /// Multiplies the value of this sample by the given amount.
    fn amplify(self, value: f32) -> Self;

    /// Calls `saturating_add` on the sample.
    fn saturating_add(self, other: Self) -> Self;

    /// Returns the value corresponding to the absence of sound.
    fn zero_value() -> Self;
}

impl Sample for u16 {
    #[inline]
    fn lerp(first: u16, second: u16, numerator: u32, denominator: u32) -> u16 {
        let a = first as i32;
        let b = second as i32;
        let n = numerator as i32;
        let d = denominator as i32;
        (a + (b - a) * n / d) as u16
    }

    #[inline]
    fn amplify(self, value: f32) -> u16 {
        ((self as f32) * value) as u16
    }

    #[inline]
    fn saturating_add(self, other: u16) -> u16 {
        self.saturating_add(other)
    }

    #[inline]
    fn zero_value() -> u16 {
        32768
    }
}

impl Sample for i16 {
    #[inline]
    fn lerp(first: i16, second: i16, numerator: u32, denominator: u32) -> i16 {
        (first as i32 + (second as i32 - first as i32) * numerator as i32 / denominator as i32)
            as i16
    }

    #[inline]
    fn amplify(self, value: f32) -> i16 {
        ((self as f32) * value) as i16
    }

    #[inline]
    fn saturating_add(self, other: i16) -> i16 {
        self.saturating_add(other)
    }

    #[inline]
    fn zero_value() -> i16 {
        0
    }
}

impl Sample for f32 {
    #[inline]
    fn lerp(first: f32, second: f32, numerator: u32, denominator: u32) -> f32 {
        first + (second - first) * numerator as f32 / denominator as f32
    }

    #[inline]
    fn amplify(self, value: f32) -> f32 {
        self * value
    }

    #[inline]
    fn saturating_add(self, other: f32) -> f32 {
        self + other
    }

    #[inline]
    fn zero_value() -> f32 {
        0.0
    }
}
