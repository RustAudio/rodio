use std::time::Duration;

use crate::{Sample, Source};

use super::SeekError;

/// Internal function that builds a `TrackPosition` object.
pub fn trackable<I>(source: I) -> TrackPosition<I> {
    TrackPosition {
        input: source,
        samples_elapsed: 0,
    }
}

#[derive(Clone, Debug)]
pub struct TrackPosition<I> {
    input: I,
    samples_elapsed: usize,
}

impl<I> TrackPosition<I> {
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

impl<I> TrackPosition<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Returns the inner source.
    #[inline]
    pub fn get_pos(&self) -> f64 {
        self.samples_elapsed as f64 / self.input.sample_rate() as f64 / self.input.channels() as f64
    }
}

impl<I> Iterator for TrackPosition<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        let item = self.input.next();
        if item.is_some() {
            self.samples_elapsed += 1;
        };
        item
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> Source for TrackPosition<I>
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

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        let result = self.input.try_seek(pos);
        if result.is_ok() {
            self.samples_elapsed = (pos.as_secs_f64()
                * self.input.sample_rate() as f64
                * self.input.channels() as f64) as usize;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::buffer::SamplesBuffer;
    use crate::source::Source;

    #[test]
    fn test_position() {
        let inner = SamplesBuffer::new(1, 1, vec![10i16, -10, 10, -10, 20, -20]);
        let mut source = inner.trackable();

        assert_eq!(source.get_pos(), 0.0);
        source.next();
        assert_eq!(source.get_pos(), 1.0);
        source.next();
        assert_eq!(source.get_pos(), 2.0);

        assert_eq!(source.try_seek(Duration::new(1, 0)).is_ok(), true);
        assert_eq!(source.get_pos(), 1.0);
    }
}
