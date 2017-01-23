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
    fn get_current_frame_len(&self) -> Option<usize> {
        self.input.get_current_frame_len()
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        self.input.get_channels()
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        self.input.get_samples_rate()
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        self.input.get_total_duration()
    }
}
