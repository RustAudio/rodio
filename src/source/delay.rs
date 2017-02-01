use std::time::Duration;

use Source;
use Sample;

/// Internal function that builds a `Delay` object.
pub fn delay<I>(input: I, duration: Duration) -> Delay<I>
    where I: Source,
          I::Item: Sample
{
    let duration_ns = duration.as_secs() * 1000000000 + duration.subsec_nanos() as u64;
    let samples = duration_ns * input.get_samples_rate() as u64 * input.get_channels() as u64 /
                  1000000000;

    Delay {
        input: input,
        remaining_samples: samples as usize,
        requested_duration: duration,
    }
}

/// A source that delays the given source by a certain amount.
#[derive(Clone, Debug)]
pub struct Delay<I>
    where I: Source,
          I::Item: Sample
{
    input: I,
    remaining_samples: usize,
    requested_duration: Duration,
}

impl<I> Iterator for Delay<I>
    where I: Source,
          I::Item: Sample
{
    type Item = <I as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        if self.remaining_samples >= 1 {
            self.remaining_samples -= 1;
            Some(Sample::zero_value())

        } else {
            self.input.next()
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (min, max) = self.input.size_hint();
        (min + self.remaining_samples, max.map(|v| v + self.remaining_samples))
    }
}

impl<I> Source for Delay<I>
    where I: Iterator + Source,
          I::Item: Sample
{
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        self.input.get_current_frame_len().map(|val| val + self.remaining_samples)
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
        self.input.get_total_duration().map(|val| val + self.requested_duration)
    }
}
