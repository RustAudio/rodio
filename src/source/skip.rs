use crate::{Sample, Source};
use std::time::Duration;

/// Internal function that builds a `SkipDuration` object.
pub fn skip_duration<I>(mut input: I, duration: Duration) -> SkipDuration<I>
where
    I: Source,
    I::Item: Sample,
{
    let duration_ns = duration.as_nanos();
    let samples_to_skip =
        duration_ns * input.sample_rate() as u128 / 1_000_000_000 * input.channels() as u128;

    for _ in 0..samples_to_skip {
        if input.next().is_none() {
            break;
        }
    }

    SkipDuration {
        input,
        skipped_duration: duration,
    }
}

/// A source that skips specified duration of the given source from it's current position.
#[derive(Clone, Debug)]
pub struct SkipDuration<I> {
    input: I,
    skipped_duration: Duration,
}

impl<I> Iterator for SkipDuration<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = <I as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.input.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> Source for SkipDuration<I>
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
        self.input.total_duration().map(|val| {
            val.checked_sub(self.skipped_duration)
                .unwrap_or(Duration::from_secs(0))
        })
    }
}
