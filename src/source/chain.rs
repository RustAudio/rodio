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
pub fn chain<I>(iterator: I) -> Chain<I::IntoIter>
where
    I: IntoIterator,
{
    let mut iterator = iterator.into_iter();
    let first_source = iterator.next();

    Chain {
        iterator,
        current_source: first_source,
    }
}

/// A source that chains sources provided by an iterator.
#[derive(Clone)]
pub struct Chain<I>
where
    I: Iterator,
{
    // The iterator that provides sources.
    iterator: I,
    // Is only ever `None` if the first element of the iterator is `None`.
    current_source: Option<I::Item>,
}

impl<I> Iterator for Chain<I>
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

            let source = self.iterator.next()?;
            self.current_source = Some(source);
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

impl<I> Source for Chain<I>
where
    I: Iterator,
    I::Item: Iterator + Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        // The transition between sources must be a span boundary. We propagate the current
        // source's span length directly. When the source is exhausted it already returns Some(0),
        // which correctly signals end-of-span. The None case (empty iterator) is likewise
        // signaled as Some(0).
        match &self.current_source {
            None => Some(0),
            Some(src) => src.current_span_len(),
        }
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
    use crate::source::{chain, Source};

    #[test]
    fn empty_chain_reports_end_of_span() {
        let c = chain(std::iter::empty::<SamplesBuffer>());
        assert_eq!(c.current_span_len(), Some(0));
    }

    #[test]
    fn exhausted_chain_reports_end_of_span() {
        let mut c = chain(std::iter::once(SamplesBuffer::new(
            nz!(1),
            nz!(48000),
            vec![1.0, 2.0],
        )));
        assert_eq!(c.next(), Some(1.0));
        assert_eq!(c.next(), Some(2.0));
        assert_eq!(c.next(), None);
        assert_eq!(c.current_span_len(), Some(0));
    }

    #[test]
    fn basic() {
        let mut rx = chain((0..2).map(|n| {
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
