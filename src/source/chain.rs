use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::math::nz;
use crate::{Sample, Source};

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
        current_ended: false,
        pending: None,
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
    // Whether `current_source`'s `next()` has returned `None`. A source
    // is never asked for a sample again once it has.
    current_ended: bool,
    // A sample from a source that reported its end but still had one.
    pending: Option<Sample>,
}

impl<I> Chain<I>
where
    I: Iterator,
    I::Item: Source,
{
    fn skip_finished_sources(&mut self) {
        while let Some(src) = &mut self.current_source {
            if !src.is_exhausted() {
                return;
            }
            // Leave a source only once its `next()` has returned `None`:
            // sources like `EmptyCallback` and `Done` act on that call.
            if let Some(value) = src.next() {
                self.pending = Some(value);
                return;
            }
            self.current_ended = true;
            let Some(next) = self.iterator.next() else {
                return;
            };
            self.current_source = Some(next);
            self.current_ended = false;
        }
    }
}

impl<I> Iterator for Chain<I>
where
    I: Iterator,
    I::Item: Iterator + Source,
{
    type Item = <I::Item as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(value) = self.pending.take() {
            self.skip_finished_sources();
            return Some(value);
        }
        loop {
            if let Some(src) = self.current_source.as_mut().filter(|_| !self.current_ended) {
                if let Some(value) = src.next() {
                    // Move on as soon as a source reports its end, so
                    // `current_span_len`, `channels` and `sample_rate`
                    // describe the next sample rather than a finished
                    // source. Its `Some(0)` would otherwise read as the
                    // end of the whole chain.
                    self.skip_finished_sources();
                    return Some(value);
                }
                self.current_ended = true;
            }

            let source = self.iterator.next()?;
            self.current_source = Some(source);
            self.current_ended = false;
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.current_source {
            None => (0, Some(0)),
            // An ended source is never asked again, whatever it now says.
            Some(_) if self.current_ended => (0, None),
            Some(cur) => (
                cur.size_hint().0 + usize::from(self.pending.is_some()),
                None,
            ),
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
        // signalled as Some(0), as is a source that has ended, whatever it now says.
        //
        // A sample a source still had after reporting its end is a span of its own: the source
        // already reports `Some(0)`, which would read as the end of the whole chain.
        if self.pending.is_some() {
            return Some(1);
        }
        match &self.current_source {
            None => Some(0),
            Some(_) if self.current_ended => Some(0),
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
            source.try_seek(pos)?;
            self.current_ended = false;
            self.pending = None;
            self.skip_finished_sources();
            Ok(())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use crate::buffer::SamplesBuffer;
    use crate::common::{ChannelCount, SampleRate};
    use crate::math::nz;
    use crate::source::{chain, Done, EmptyCallback, SeekError, Source};
    use crate::Sample;

    fn counting_callback(count: &Arc<AtomicUsize>) -> EmptyCallback {
        let count = count.clone();
        EmptyCallback::new(Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        }))
    }

    #[test]
    fn calls_an_empty_callback_between_sources_once() {
        let count = Arc::new(AtomicUsize::new(0));
        let sources: Vec<Box<dyn Source + Send>> = vec![
            Box::new(SamplesBuffer::new(nz!(1), nz!(48000), vec![1.0, 2.0])),
            Box::new(counting_callback(&count)),
            Box::new(SamplesBuffer::new(nz!(1), nz!(48000), vec![3.0])),
        ];
        let mut c = chain(sources);
        assert_eq!(c.next(), Some(1.0));
        assert_eq!(count.load(Ordering::SeqCst), 0);
        assert_eq!(c.next(), Some(2.0));
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "the first source has ended"
        );
        assert_eq!(c.next(), Some(3.0));
        assert_eq!(c.next(), None);
        assert_eq!(c.next(), None);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn calls_a_trailing_empty_callback_once() {
        let count = Arc::new(AtomicUsize::new(0));
        let sources: Vec<Box<dyn Source + Send>> = vec![
            Box::new(SamplesBuffer::new(nz!(1), nz!(48000), vec![1.0])),
            Box::new(counting_callback(&count)),
        ];
        let mut c = chain(sources);
        assert_eq!(c.next(), Some(1.0));
        assert_eq!(c.next(), None);
        assert_eq!(c.next(), None);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn tells_a_source_it_ended_before_moving_on() {
        let count = Arc::new(AtomicUsize::new(0));
        let ended = count.clone();
        let done = Done::new(
            SamplesBuffer::new(nz!(1), nz!(48000), vec![1.0, 2.0]),
            move |_: &mut SamplesBuffer| {
                ended.fetch_add(1, Ordering::SeqCst);
            },
        );
        let mut c = chain([
            Box::new(done) as Box<dyn Source + Send>,
            Box::new(SamplesBuffer::new(nz!(1), nz!(48000), vec![3.0])),
        ]);
        assert_eq!(c.by_ref().take(2).collect::<Vec<_>>(), [1.0, 2.0]);
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "the first source has ended"
        );
        assert_eq!(c.collect::<Vec<_>>(), [3.0]);
    }

    #[test]
    fn plays_the_last_source_again_after_seeking_back() {
        let mut c = chain([SamplesBuffer::new(nz!(1), nz!(48000), vec![1.0, 2.0])]);
        assert_eq!(c.by_ref().collect::<Vec<_>>(), [1.0, 2.0]);
        c.try_seek(Duration::ZERO).unwrap();
        assert_eq!(c.collect::<Vec<_>>(), [1.0, 2.0]);
    }

    /// Reports its end one sample early.
    struct EndsEarly(Vec<Sample>);

    impl Iterator for EndsEarly {
        type Item = Sample;
        fn next(&mut self) -> Option<Sample> {
            (!self.0.is_empty()).then(|| self.0.remove(0))
        }
    }

    impl Source for EndsEarly {
        fn current_span_len(&self) -> Option<usize> {
            Some(self.0.len().saturating_sub(1))
        }
        fn channels(&self) -> ChannelCount {
            nz!(1)
        }
        fn sample_rate(&self) -> SampleRate {
            nz!(48000)
        }
        fn total_duration(&self) -> Option<Duration> {
            None
        }
        fn try_seek(&mut self, _: Duration) -> Result<(), SeekError> {
            Ok(())
        }
    }

    #[test]
    fn keeps_samples_a_source_has_after_reporting_its_end() {
        let c = chain([
            Box::new(EndsEarly(vec![1.0, 2.0, 3.0])) as Box<dyn Source + Send>,
            Box::new(SamplesBuffer::new(nz!(1), nz!(48000), vec![4.0])),
        ]);
        assert_eq!(c.collect::<Vec<_>>(), [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn reports_a_pending_sample_as_a_span_of_its_own() {
        // Cut after one sample, so `take_duration` pads the frame with a
        // sample after reporting its end.
        let cut = || {
            SamplesBuffer::new(nz!(2), nz!(48000), vec![1.0, 2.0])
                .take_duration(Duration::from_micros(15))
        };
        let sources = || -> [Box<dyn Source + Send>; 2] {
            [
                Box::new(cut()),
                Box::new(SamplesBuffer::new(nz!(2), nz!(48000), vec![3.0, 4.0])),
            ]
        };
        let direct: Vec<_> = chain(sources()).collect();
        assert_eq!(direct, [1.0, 0.0, 3.0, 4.0]);

        let mut c = chain(sources());
        assert_eq!(c.next(), Some(1.0));
        assert_eq!(
            c.current_span_len(),
            Some(1),
            "the padding sample is pending"
        );
        assert!(!c.is_exhausted());
        assert_eq!(chain(sources()).buffered().collect::<Vec<_>>(), direct);
    }

    #[test]
    fn stays_ended_when_the_last_source_rewinds_itself() {
        let done = Done::new(
            SamplesBuffer::new(nz!(1), nz!(48000), vec![1.0]),
            |buffer: &mut SamplesBuffer| buffer.try_seek(Duration::ZERO).unwrap(),
        );
        let mut c = chain([done]);
        assert_eq!(c.next(), Some(1.0));
        assert_eq!(c.next(), None);
        assert_eq!(c.size_hint(), (0, None));
        assert_eq!(c.current_span_len(), Some(0));
        assert!(c.is_exhausted());
        assert_eq!(c.next(), None);
    }

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
    fn reports_next_source_once_one_ends() {
        let mut c = chain([
            SamplesBuffer::new(nz!(1), nz!(24000), vec![1.0, 2.0]),
            SamplesBuffer::new(nz!(1), nz!(24000), vec![]),
            SamplesBuffer::new(nz!(1), nz!(24000), vec![3.0]),
        ]);
        c.next();
        c.next();
        assert!(!c.is_exhausted(), "a finished source isn't the chain's end");
        assert_eq!(c.current_span_len(), Some(1));
        assert_eq!(c.collect::<Vec<_>>(), [3.0]);
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
        assert_eq!(rx.channels(), nz!(2));
        assert_eq!(rx.sample_rate().get(), 96000);
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), None);
    }
}
