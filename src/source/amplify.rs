use std::time::Duration;

use crate::Sample;
use crate::Source;

/// Internal function that builds a `Amplify` object.
pub fn amplify<I>(input: I, factor: f32) -> Amplify<I>
where
    I: Source,
    I::Item: Sample,
{
    Amplify {
        input: input,
        factor: factor,
    }
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
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        self.input.next().map(|value| value.amplify(self.factor))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for Amplify<I>
where
    I: Source + ExactSizeIterator,
    I::Item: Sample,
{
}

impl<I> Source for Amplify<I>
where
    I: Source,
    I::Item: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        self.input.current_frame_len()
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.input.channels()
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
}
