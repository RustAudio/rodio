use std::time::Duration;

use crate::Sample;
use crate::Source;

/// Internal function that builds a `Pausable` object.
pub fn pausable<I>(source: I, paused: bool) -> Pausable<I>
where
    I: Source,
    I::Item: Sample,
{
    let paused_channels = if paused {
        Some(source.channels())
    } else {
        None
    };
    Pausable {
        input: source,
        paused_channels,
        remaining_paused_samples: 0,
    }
}

#[derive(Clone, Debug)]
pub struct Pausable<I> {
    input: I,
    paused_channels: Option<u16>,
    remaining_paused_samples: u16,
}

impl<I> Pausable<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Sets whether the filter applies.
    ///
    /// If set to true, the inner sound stops playing and no samples are processed from it.
    #[inline]
    pub fn set_paused(&mut self, paused: bool) {
        match (self.paused_channels, paused) {
            (None, true) => self.paused_channels = Some(self.input.channels()),
            (Some(_), false) => self.paused_channels = None,
            _ => (),
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
}

impl<I> Iterator for Pausable<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        if self.remaining_paused_samples > 0 {
            self.remaining_paused_samples -= 1;
            return Some(I::Item::zero_value());
        }

        if let Some(paused_channels) = self.paused_channels {
            self.remaining_paused_samples = paused_channels - 1;
            return Some(I::Item::zero_value());
        }

        self.input.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> Source for Pausable<I>
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
