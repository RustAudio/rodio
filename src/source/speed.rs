use std::time::Duration;

use Sample;
use Source;

/// Internal function that builds a `Speed` object.
pub fn speed<I>(input: I, factor: f32) -> Speed<I>
    where I: Source,
          I::Item: Sample
{
    Speed {
        input: input,
        factor: factor,
    }
}

/// Filter that modifies each sample by a given value.
#[derive(Clone, Debug)]
pub struct Speed<I>
    where I: Source,
          I::Item: Sample
{
    input: I,
    factor: f32,
}

impl<I> Iterator for Speed<I>
    where I: Source,
          I::Item: Sample
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

impl<I> ExactSizeIterator for Speed<I>
    where I: Source + ExactSizeIterator,
          I::Item: Sample
{
}

impl<I> Source for Speed<I>
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
        (self.input.get_samples_rate() as f32 * self.factor) as u32
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        // TODO: the crappy API of duration makes this code difficult to write
        if let Some(duration) = self.input.get_total_duration() {
            let as_ns = duration.as_secs() * 1000000000 + duration.subsec_nanos() as u64;
            let new_val = (as_ns as f32 / self.factor) as u64;
            Some(Duration::new(new_val / 1000000000, (new_val % 1000000000) as u32))

        } else {
            None
        }
    }
}
