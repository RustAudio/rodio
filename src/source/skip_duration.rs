use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::math::{PrevMultipleOf, NS_PER_SECOND};
use crate::Source;

/// A source that skips specified duration of the given source from it's current position.
#[derive(Clone, Debug)]
pub struct SkipDuration<I> {
    input: I,
    skipped_duration: Duration,
}

impl<I> SkipDuration<I> {
    pub(crate) fn new(mut input: I, duration: Duration) -> SkipDuration<I>
    where
        I: Source,
    {
        do_skip_duration(&mut input, duration);
        Self {
            input,
            skipped_duration: duration,
        }
    }
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
///
/// # Panics
/// If trying to skip more than 584 days ahead. If you need that functionality
/// please open an issue I would love to know why (and help out).
fn do_skip_duration<I>(input: &mut I, duration: Duration)
where
    I: Source,
{
    // `u64::MAX` can store 584 days of nanosecond precision time. To not be off by
    // a single sample (that would be regression) we first multiply the time by
    // `samples_per second`. Which for a 96kHz 10 channel audio stream is
    // 960_000 samples. That would only leave 0.87 hour of potential skip time. Hence
    // we use an `u128` to calculate samples to skip.
    let mut duration: u64 = duration
        .as_nanos()
        .try_into()
        .expect("can not skip more then 584 days of audio");
    let mut ns_per_frame: u64 = 0;

    while duration > ns_per_frame {
        assert!(input.sample_rate() > 0);
        assert!(input.channels() > 0);

        ns_per_frame = NS_PER_SECOND / input.sample_rate() as u64;

        let samples_per_second = input.sample_rate() as u64 * input.channels() as u64;
        let samples_to_skip =
            (duration as u128 * samples_per_second as u128 / NS_PER_SECOND as u128) as u64;
        let samples_to_skip = samples_to_skip.prev_multiple_of(input.channels());

        let mut skipped = 0;
        while skipped < samples_to_skip {
            if input.next().is_none() {
                return;
            }
            skipped += 1;
            if input.parameters_changed() {
                break;
            }
        }

        duration -= skipped * NS_PER_SECOND / samples_per_second;
    }
}
