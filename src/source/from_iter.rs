use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::math::nz;
use crate::Source;

/// Builds a source that chains sources provided by an iterator.
///
/// The `iterator` parameter is an iterator that produces a source. The source is then played.
/// Whenever the source ends, the `iterator` is used again in order to produce the source that is
/// played next.
///
/// If the `iterator` produces `None`, then the sound ends.
pub fn from_iter<I>(iterator: I) -> FromIter<I::IntoIter>
where
    I: IntoIterator,
{
    let mut iterator = iterator.into_iter();
    let first_source = iterator.next();

    FromIter {
        iterator,
        current_source: first_source,
    }
}

/// A source that chains sources provided by an iterator.
#[derive(Clone)]
pub struct FromIter<I>
where
    I: Iterator,
{
    // The iterator that provides sources.
    iterator: I,
    // Is only ever `None` if the first element of the iterator is `None`.
    current_source: Option<I::Item>,
}

impl<I> Iterator for FromIter<I>
where
    I: Iterator,
    I::Item: Iterator + Source,
{
    type Item = <I::Item as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(src) = &mut self.current_source {
                if let Some(value) = src.next() {
                    return Some(value);
                }
            }

            if let Some(src) = self.iterator.next() {
                self.current_source = Some(src);
            } else {
                return None;
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if let Some(cur) = &self.current_source {
            (cur.size_hint().0, None)
        } else {
            (0, Some(0))
        }
    }
}

impl<I> Source for FromIter<I>
where
    I: Iterator,
    I::Item: Iterator + Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        if let Some(src) = &self.current_source {
            if !src.is_exhausted() {
                return src.current_span_len();
            }
        }

        None
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        if let Some(src) = &self.current_source {
            src.channels()
        } else {
            // Dummy value that only happens if the iterator was empty.
            nz!(2)
        }
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        if let Some(src) = &self.current_source {
            src.sample_rate()
        } else {
            // Dummy value that only happens if the iterator was empty.
            nz!(44100)
        }
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        if let Some(source) = self.current_source.as_mut() {
            source.try_seek(pos)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::buffer::SamplesBuffer;
    use crate::math::nz;
    use crate::source::{from_iter, Source};

    #[test]
    fn basic() {
        let mut rx = from_iter((0..2).map(|n| {
            if n == 0 {
                SamplesBuffer::new(nz!(1), nz!(48000), vec![10.0, -10.0, 10.0, -10.0])
            } else if n == 1 {
                SamplesBuffer::new(nz!(2), nz!(96000), vec![5.0, 5.0, 5.0, 5.0])
            } else {
                unreachable!()
            }
        }));

        assert_eq!(rx.channels(), nz!(1));
        assert_eq!(rx.sample_rate().get(), 48000);
        assert_eq!(rx.next(), Some(10.0));
        assert_eq!(rx.next(), Some(-10.0));
        assert_eq!(rx.next(), Some(10.0));
        assert_eq!(rx.next(), Some(-10.0));
        /*assert_eq!(rx.channels(), 2);
        assert_eq!(rx.sample_rate().get(), 96000);*/
        // FIXME: not working
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), None);
    }
}
