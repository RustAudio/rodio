use std::time::Duration;

use crate::source::buffered::Buffered;

use crate::Sample;
use crate::Source;

/// Internal function that builds a `Repeat` object.
pub fn repeat<I>(input: I) -> Repeat<I>
where
    I: Source,
    I::Item: Sample,
{
    let input = input.buffered();
    Repeat {
        inner: input.clone(),
        next: input,
    }
}

/// A source that repeats the given source.
pub struct Repeat<I>
where
    I: Source,
    I::Item: Sample,
{
    inner: Buffered<I>,
    next: Buffered<I>,
}

impl<I> Iterator for Repeat<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = <I as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        if let Some(value) = self.inner.next() {
            return Some(value);
        }

        self.inner = self.next.clone();
        self.inner.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        // infinite
        (0, None)
    }
}

impl<I> Source for Repeat<I>
where
    I: Iterator + Source,
    I::Item: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        match self.inner.current_frame_len() {
            Some(0) => self.next.current_frame_len(),
            a => a,
        }
    }

    #[inline]
    fn channels(&self) -> u16 {
        match self.inner.current_frame_len() {
            Some(0) => self.next.channels(),
            _ => self.inner.channels(),
        }
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        match self.inner.current_frame_len() {
            Some(0) => self.next.sample_rate(),
            _ => self.inner.sample_rate(),
        }
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl<I> Clone for Repeat<I>
where
    I: Source,
    I::Item: Sample,
{
    #[inline]
    fn clone(&self) -> Repeat<I> {
        Repeat {
            inner: self.inner.clone(),
            next: self.next.clone(),
        }
    }
}
