use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::math::{PrevMultipleOfNonZero, NS_PER_SECOND};
use crate::Source;

/// A source that truncates the given source to a certain duration.
#[derive(Clone, Debug)]
pub struct TakeDuration<I> {
    input: I,
    requested_duration: Duration,
    remaining_ns: u64,
    samples_per_second: u64,
    samples_to_take: u64,
    samples_taken: u64,
    fadeout: bool,
}

impl<I> TakeDuration<I>
where
    I: Source,
{
    pub(crate) fn new(input: I, duration: Duration) -> TakeDuration<I>
    where
        I: Source,
    {
        let remaining_ns: u64 = duration
            .as_nanos()
            .try_into()
            .expect("can not take more then 584 days of audio");

        let samples_per_second = input.sample_rate() as u64 * input.channels().get() as u64;
        let samples_to_take =
            (remaining_ns as u128 * samples_per_second as u128 / NS_PER_SECOND as u128) as u64;
        let samples_to_take = samples_to_take.prev_multiple_of(input.channels());

        Self {
            input,
            remaining_ns,
            fadeout: false,
            samples_per_second,
            samples_to_take,
            samples_taken: 0,
            requested_duration: duration,
        }
    }

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

    /// Optionally the truncated source end with a FadeOut. The fade-out
    /// covers the entire length of the take source.
    pub fn fadeout(&mut self, enabled: bool) {
        self.fadeout = enabled;
    }

    /// Make the truncated source end with a FadeOut. The fade-out covers the
    /// entire length of the take source.
    pub fn with_fadeout(mut self, enabled: bool) -> Self {
        self.fadeout = enabled;
        self
    }

    /// Remove any filter set.
    pub fn clear_filter(&mut self) {
        self.fadeout = false;
    }

    fn next_samples_per_second(&mut self) -> u64 {
        self.input.sample_rate() as u64 * self.input.channels().get() as u64
    }

    fn next_samples_to_take(&mut self) -> u64 {
        let samples_to_take = (self.remaining_ns as u128 * self.samples_per_second as u128
            / NS_PER_SECOND as u128) as u64;
        let samples_to_take = samples_to_take.prev_multiple_of(self.input.channels());
        samples_to_take
    }

    fn duration_taken(&mut self) -> u64 {
        self.samples_taken * NS_PER_SECOND / self.samples_per_second
    }
}

impl<I> Iterator for TakeDuration<I>
where
    I: Source,
{
    type Item = <I as Iterator>::Item;

    // implementation is adapted of skip_duration
    //
    // if tuples are frames you could define fadeout as this:
    //    [(1.0, 1.0), (1.0, 1.0), (1.0, 1.0)]
    // -> [(1.0, 1.0), (0.5, 0.5), (0.0, 0.0)]
    // instead because its simpler, faster and what previous rodio versions did we do:
    //    [(1.0, 1.0), (1.0, 1.0), (1.0, 1.0)]
    // -> [(1.0, .83), (.66, 0.5), (.33, .16)]
    // at normal sample_rates you do not hear a difference
    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        if self.input.parameters_changed() {
            self.remaining_ns -= self.duration_taken();
            self.samples_per_second = self.next_samples_per_second();
            self.samples_to_take = self.next_samples_to_take();
            self.samples_taken = 0;
        }

        if self.samples_taken >= self.samples_to_take {
            return None;
        }

        let Some(sample) = self.input.next() else {
            return None;
        };

        let ret = if self.fadeout {
            let total = self.requested_duration.as_nanos() as u64;
            let remaining = self.remaining_ns - self.duration_taken();
            Some(sample * remaining as f32 / total as f32)
        } else {
            Some(sample)
        };
        self.samples_taken += 1;
        ret
    }
}

// TODO: size_hint

impl<I> Source for TakeDuration<I>
where
    I: Iterator + Source,
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
        if let Some(duration) = self.input.total_duration() {
            if duration < self.requested_duration {
                Some(duration)
            } else {
                Some(self.requested_duration)
            }
        } else {
            None
        }
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.input.try_seek(pos)
    }
}
