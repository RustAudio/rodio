use std::time::Duration;

use crate::{Sample, Source};

use super::{linear_ramp::linear_gain_ramp, LinearGainRamp, SeekError};

/// Internal function that builds a `FadeIn` object.
pub fn fadein<I>(input: I, duration: Duration) -> FadeIn<I>
where
    I: Source,
    I::Item: Sample,
{
    FadeIn {
        input: linear_gain_ramp(input, duration, 0.0f32, 1.0f32, false),
    }
}

/// Filter that modifies raises the volume from silence over a time period.
#[derive(Clone, Debug)]
pub struct FadeIn<I> {
    input: LinearGainRamp<I>,
}

impl<I> FadeIn<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Returns a reference to the inner source.
    #[inline]
    pub fn inner(&self) -> &I {
        self.input.inner()
    }

    /// Returns a mutable reference to the inner source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        self.input.inner_mut()
    }

    /// Returns the inner source.
    #[inline]
    pub fn into_inner(self) -> I {
        self.input.into_inner()
    }
}

impl<I> Iterator for FadeIn<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        self.input.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for FadeIn<I>
where
    I: Source + ExactSizeIterator,
    I::Item: Sample,
{
}

impl<I> Source for FadeIn<I>
where
    I: Source,
    I::Item: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        self.inner().current_frame_len()
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.inner().channels()
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.inner().sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.inner().total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner_mut().try_seek(pos)
    }
}
