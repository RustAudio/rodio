use std::time::Duration;

use Sample;
use Source;

/// Internal function that builds a `FadeIn` object.
pub fn fadein<I>(input: I, duration: Duration) -> FadeIn<I>
    where I: Source,
          I::Item: Sample
{
    let duration = duration.as_secs() * 1000000000 + duration.subsec_nanos() as u64;

    FadeIn {
        input: input,
        remaining_ns: duration as f32,
        total_ns: duration as f32,
    }
}

/// Filter that modifies each sample by a given value.
#[derive(Clone, Debug)]
pub struct FadeIn<I>
    where I: Source,
          I::Item: Sample
{
    input: I,
    remaining_ns: f32,
    total_ns: f32,
}

impl<I> Iterator for FadeIn<I>
    where I: Source,
          I::Item: Sample
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        if self.remaining_ns <= 0.0 {
            return self.input.next();
        }

        let factor = 1.0 - self.remaining_ns / self.total_ns;
        self.remaining_ns -= 1000000000.0 /
                             (self.input.get_samples_rate() as f32 * self.get_channels() as f32);
        self.input.next().map(|value| value.amplify(factor))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for FadeIn<I>
    where I: Source + ExactSizeIterator,
          I::Item: Sample
{
}

impl<I> Source for FadeIn<I>
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
