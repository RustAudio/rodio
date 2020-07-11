use std::time::Duration;

use crate::Sample;
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
        iterator: iterator,
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
    <I::Item as Iterator>::Item: Sample,
{
    type Item = <I::Item as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<<I::Item as Iterator>::Item> {
        loop {
            if let Some(ref mut src) = self.current_source {
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
        if let Some(ref cur) = self.current_source {
            (cur.size_hint().0, None)
        } else {
            (0, None)
        }
    }
}

impl<I> Source for FromIter<I>
where
    I: Iterator,
    I::Item: Iterator + Source,
    <I::Item as Iterator>::Item: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        // This function is non-trivial because the boundary between the current source and the
        // next must be a frame boundary as well.
        //
        // The current sound is free to return `None` for `current_frame_len()`, in which case
        // we *should* return the number of samples remaining the current sound.
        // This can be estimated with `size_hint()`.
        //
        // If the `size_hint` is `None` as well, we are in the worst case scenario. To handle this
        // situation we force a frame to have a maximum number of samples indicate by this
        // constant.
        const THRESHOLD: usize = 10240;

        // Try the current `current_frame_len`.
        if let Some(ref src) = self.current_source {
            if let Some(val) = src.current_frame_len() {
                if val != 0 {
                    return Some(val);
                }
            }
        }

        // Try the size hint.
        if let Some(ref src) = self.current_source {
            if let Some(val) = src.size_hint().1 {
                if val < THRESHOLD && val != 0 {
                    return Some(val);
                }
            }
        }

        // Otherwise we use the constant value.
        Some(THRESHOLD)
    }

    #[inline]
    fn channels(&self) -> u16 {
        if let Some(ref src) = self.current_source {
            src.channels()
        } else {
            // Dummy value that only happens if the iterator was empty.
            2
        }
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        if let Some(ref src) = self.current_source {
            src.sample_rate()
        } else {
            // Dummy value that only happens if the iterator was empty.
            44100
        }
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::buffer::SamplesBuffer;
    use crate::source::from_iter;
    use crate::source::Source;

    #[test]
    fn basic() {
        let mut rx = from_iter((0..2).map(|n| {
            if n == 0 {
                SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10])
            } else if n == 1 {
                SamplesBuffer::new(2, 96000, vec![5i16, 5, 5, 5])
            } else {
                unreachable!()
            }
        }));

        assert_eq!(rx.channels(), 1);
        assert_eq!(rx.sample_rate(), 48000);
        assert_eq!(rx.next(), Some(10));
        assert_eq!(rx.next(), Some(-10));
        assert_eq!(rx.next(), Some(10));
        assert_eq!(rx.next(), Some(-10));
        /*assert_eq!(rx.channels(), 2);
        assert_eq!(rx.sample_rate(), 96000);*/
        // FIXME: not working
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), None);
    }
}
