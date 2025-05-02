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
    f32::powf(10f32, decibels * 0.05)
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
        for (db, linear) in DECIBELS_LINEAR_TABLE {
            const PRECISION: f32 = 5.066e3;
            let to_linear = to_linear(db);

            assert!(
                2.0 * (to_linear - linear).abs()
                    < PRECISION * f32::EPSILON * (to_linear.abs() + linear.abs())
            );
        }
    }
}
