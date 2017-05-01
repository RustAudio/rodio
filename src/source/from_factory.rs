use std::time::Duration;

use Sample;
use Source;

/// Builds a source that chains sources built from a factory.
///
/// The `factory` parameter is a function that produces a source. The source is then played.
/// Whenever the source ends, `factory` is called again in order to produce the source that is
/// played next.
///
/// If the `factory` closure returns `None`, then the sound ends.
pub fn from_factory<F, S>(mut factory: F) -> FromFactory<F, S>
    where F: FnMut() -> Option<S>
{
    let first_source = factory().expect("The factory returned an empty source");    // TODO: meh

    FromFactory {
        factory: factory,
        current_source: first_source,
    }
}

/// A source that chains sources built from a factory.
#[derive(Clone)]
pub struct FromFactory<F, S> {
    factory: F,
    current_source: S,
}

impl<F, S> Iterator for FromFactory<F, S>
    where F: FnMut() -> Option<S>,
          S: Source,
          S::Item: Sample
{
    type Item = <S as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<<S as Iterator>::Item> {
        loop {
            if let Some(value) = self.current_source.next() {
                return Some(value);
            }

            if let Some(src) = (self.factory)() {
                self.current_source = src;
            } else {
                return None;
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.current_source.size_hint().0, None)
    }
}

impl<F, S> Source for FromFactory<F, S>
    where F: FnMut() -> Option<S>,
          S: Iterator + Source,
          S::Item: Sample
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
        if let Some(val) = self.current_source.current_frame_len() {
            if val != 0 {
                return Some(val);
            }
        }

        // Try the size hint.
        if let Some(val) = self.current_source.size_hint().1 {
            if val < THRESHOLD && val != 0 {
                return Some(val);
            }
        }

        // Otherwise we use the constant value.
        Some(THRESHOLD)
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.current_source.channels()
    }

    #[inline]
    fn samples_rate(&self) -> u32 {
        self.current_source.samples_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use buffer::SamplesBuffer;
    use source::from_factory;
    use source::Source;

    #[test]
    fn basic() {
        let mut n = 0;
        let mut rx = from_factory(move || {
            if n == 0 {
                n = 1;
                Some(SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]))
            } else if n == 1 {
                n = 2;
                Some(SamplesBuffer::new(2, 96000, vec![5i16, 5, 5, 5]))
            } else {
                None
            }
        });

        assert_eq!(rx.channels(), 1);
        assert_eq!(rx.samples_rate(), 48000);
        assert_eq!(rx.next(), Some(10));
        assert_eq!(rx.next(), Some(-10));
        assert_eq!(rx.next(), Some(10));
        assert_eq!(rx.next(), Some(-10));
        /*assert_eq!(rx.channels(), 2);
        assert_eq!(rx.samples_rate(), 96000);*/     // FIXME: not working
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), None);
    }
}
