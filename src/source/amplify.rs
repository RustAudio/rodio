use std::time::Duration;

use Sample;
use Source;

/// Internal function that builds a `Amplify` object.
///
/// # Panic
///
/// Panics if `denominator` is 0.
///
pub fn amplify<I>(input: I, numerator: u32, denominator: u32) -> Amplify<I>
    where I: Source,
          I::Item: Sample
{
    assert_ne!(denominator, 0);

    Amplify {
        input: input,
        numerator: numerator,
        denominator: denominator,
    }
}

/// Filter that modifies each sample by a given value.
#[derive(Clone, Debug)]
pub struct Amplify<I>
    where I: Source,
          I::Item: Sample
{
    input: I,
    numerator: u32,
    denominator: u32,
}

impl<I> Iterator for Amplify<I>
    where I: Source,
          I::Item: Sample
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        self.input.next().map(|value| {
            Sample::lerp(value, Sample::zero_value(), self.numerator, self.denominator)
        })
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
