use std::time::Duration;

use crate::Sample;
use crate::Source;

/// Internal function that builds a `Skippable` object.
pub fn skippable<I>(source: I) -> Skippable<I> {
    Skippable {
        input: source,
        do_skip: false,
    }
}

#[derive(Clone, Debug)]
pub struct Skippable<I> {
    input: I,
    do_skip: bool,
}

impl<I> Skippable<I> {
    /// Skips the current source
    #[inline]
    pub fn skip(&mut self) {
        self.do_skip = true;
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

impl<I> Iterator for Skippable<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        if self.do_skip {
            None
        } else {
            self.input.next()
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> Source for Skippable<I>
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
