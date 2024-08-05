use std::time::Duration;

use crate::{Sample, Source};

use super::SeekError;

fn remaining_samples(until_playback: Duration, sample_rate: u32, channels: u16) -> usize {
    let ns = until_playback.as_secs() * 1_000_000_000 + until_playback.subsec_nanos() as u64;
    let samples = ns * channels as u64 * sample_rate as u64 / 1_000_000_000;
    samples as usize
}

/// Internal function that builds a `Delay` object.
pub fn delay<I>(input: I, duration: Duration) -> Delay<I>
where
    I: Source,
    I::Item: Sample,
{
    Delay {
        remaining_samples: remaining_samples(duration, input.sample_rate(), input.channels()),
        requested_duration: duration,
        input,
    }
}

/// A source that delays the given source by a certain amount.
#[derive(Clone, Debug)]
pub struct Delay<I> {
    input: I,
    remaining_samples: usize,
    requested_duration: Duration,
}

impl<I> Delay<I>
where
    I: Source,
    I::Item: Sample,
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

impl<I> Iterator for Delay<I>
where
    I: Source,
    I::Item: Sample,
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
        (
            min + self.remaining_samples,
            max.map(|v| v + self.remaining_samples),
        )
    }
}

impl<I> Source for Delay<I>
where
    I: Iterator + Source,
    I::Item: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        self.input
            .current_frame_len()
            .map(|val| val + self.remaining_samples)
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
        self.input
            .total_duration()
            .map(|val| val + self.requested_duration)
    }

    /// Pos is seen from the perspective of the api user.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::time::Duration;
    ///
    /// let mut source = inner_source.delay(Duration::from_secs(10));
    /// source.try_seek(Duration::from_secs(15));
    ///
    /// // inner_source is now at pos: Duration::from_secs(5);
    /// ```
    ///
    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        if pos < self.requested_duration {
            self.input.try_seek(Duration::ZERO)?;
            let until_playback = self.requested_duration - pos;
            self.remaining_samples =
                remaining_samples(until_playback, self.sample_rate(), self.channels());
        }
        let compensated_for_delay = pos.saturating_sub(self.requested_duration);
        self.input.try_seek(compensated_for_delay)
    }
}
