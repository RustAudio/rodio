use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::Source;

const US_PER_SECOND: u64 = 1_000_000;

/// Internal function that builds a `SkipDuration` object.
pub fn skip_duration<I>(mut input: I, duration: Duration) -> SkipDuration<I>
where
    I: Source,
{
    do_skip_duration(&mut input, duration);
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

impl<I> SkipDuration<I>
where
    I: Source,
{
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

impl<I> Iterator for SkipDuration<I>
where
    I: Source,
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
        self.input.total_duration().map(|val| {
            val.checked_sub(self.skipped_duration)
                .unwrap_or_else(|| Duration::from_secs(0))
        })
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.input.try_seek(pos)
    }
}

/// Skips specified `duration` of the given `input` source from it's current position.
fn do_skip_duration<I>(input: &mut I, mut duration: Duration)
where
    I: Source,
{
    while !duration.is_zero() {
        let us_per_sample: u64 =
            US_PER_SECOND / input.sample_rate() as u64 / input.channels() as u64;
        let mut samples_to_skip = duration.as_micros() as u64 / us_per_sample;

        while samples_to_skip > 0 && !input.parameters_changed() {
            samples_to_skip -= 1;
            if input.next().is_none() {
                return;
            }
        }

        if samples_to_skip == 0 {
            return;
        } else {
            duration -= Duration::from_micros(samples_to_skip * us_per_sample);
        }
    }
}
