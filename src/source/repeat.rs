use std::time::Duration;

use crate::source::buffered::Buffered;

use super::peekable::PeekableSource;
use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::Source;

/// A source that repeats the given source.
pub struct Repeat<I>
where
    I: Source,
{
    inner: PeekableSource<Buffered<I>>,
    next: Buffered<I>,
}

impl<I: Source> Repeat<I> {
    pub(crate) fn new(input: I) -> Repeat<I> {
        let input = input.buffered();
        Repeat {
            inner: PeekableSource::new(input.clone()),
            next: input,
        }
    }
}

impl<I: Source> Iterator for Repeat<I> {
    type Item = <I as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        if let Some(value) = self.inner.next() {
            Some(value)
        } else {
            self.inner = PeekableSource::new(self.next.clone());
            self.inner.next()
        }
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
{
    #[inline]
    fn parameters_changed(&self) -> bool {
        if self.inner.peek_next().is_none() {
            true // back to beginning of source source
        } else {
            self.inner.parameters_changed()
        }
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        if self.inner.peek_next().is_none() {
            self.next.channels()
        } else {
            self.inner.channels()
        }
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        if self.inner.peek_next().is_none() {
            self.next.sample_rate()
        } else {
            self.inner.sample_rate()
        }
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)
    }
}

impl<I> Clone for Repeat<I>
where
    I: Source,
{
    #[inline]
    fn clone(&self) -> Repeat<I> {
        Repeat {
            inner: self.inner.clone(),
            next: self.next.clone(),
        }
    }
}
