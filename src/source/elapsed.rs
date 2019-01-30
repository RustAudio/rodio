use std::time::Duration;
use std::sync::{Mutex, Arc};

use Sample;
use Source;

/// Internal function that builds a `Elapsed` object.
pub fn elapsed<I>(input: I, duration: Arc<Mutex<Duration>>) -> Elapsed<I>
where
    I: Source,
    I::Item: Sample,
{
    Elapsed {
        input: input,
        duration: duration,
    }
}

/// Filter that updates a `Duration` with the current elapsed time.
#[derive(Clone, Debug)]
pub struct Elapsed<I> {
    input: I,
    duration: Arc<Mutex<Duration>>,
}

impl<I> Iterator for Elapsed<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        let mut duration = self.duration.lock().unwrap();

        // Calculate sample_time in nanoseconds
        let sample_time = (1_000_000_000 / self.sample_rate()) / self.channels() as u32;

        let time_elapsed = Duration::from_nanos(sample_time as u64);
        *duration += time_elapsed;

        self.input.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> Source for Elapsed<I>
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
