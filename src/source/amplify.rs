use std::time::Duration;

use Sample;
use Source;

/// Internal function that builds a `Amplify` object.
pub fn amplify<I>(input: I, factor: f32) -> Amplify<I>
    where I: Source,
          I::Item: Sample
{
    Amplify {
        input: input,
        factor: factor,
    }
}

/// Filter that modifies each sample by a given value.
#[derive(Clone, Debug)]
pub struct Amplify<I>
    where I: Source,
          I::Item: Sample
{
    input: I,
    factor: f32,
}

impl<I> Iterator for Amplify<I>
    where I: Source,
          I::Item: Sample
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
    where I: Source + ExactSizeIterator,
          I::Item: Sample
{
}

impl<I> Source for Amplify<I>
    where I: Source,
          I::Item: Sample
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
    fn samples_rate(&self) -> u32 {
        self.input.samples_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
}
