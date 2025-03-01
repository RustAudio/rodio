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
    fn parameters_changed(&self) -> bool {
        self.input.parameters_changed()
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
