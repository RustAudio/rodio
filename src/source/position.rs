use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use crate::{sink::AtomicF64, Sample, Source};

use super::SeekError;

/// Internal function that builds a `TrackPosition` object.
pub fn trackable<I>(source: I, position: Arc<AtomicF64>) -> TrackPosition<I> {
    TrackPosition {
        input: source,
        samples_counted: 0,
        offset_duration: 0.0,
        position,
        current_frame_sample_rate: 0,
        current_frame_channels: 0,
        current_frame_len: None,
    }
}

#[derive(Debug)]
pub struct TrackPosition<I> {
    input: I,
    samples_counted: usize,
    offset_duration: f64,
    position: Arc<AtomicF64>,
    current_frame_sample_rate: u32,
    current_frame_channels: u16,
    current_frame_len: Option<usize>,
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
    /// Returns the position of the source.
    #[inline]
    fn get_pos(&self) -> f64 {
        self.samples_counted as f64 / self.input.sample_rate() as f64 / self.input.channels() as f64
            + self.offset_duration
    }

    #[inline]
    fn set_current_frame(&mut self) {
        self.current_frame_len = self.current_frame_len();
        self.current_frame_sample_rate = self.sample_rate();
        self.current_frame_channels = self.channels();
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
        // This should only be executed once at the first call to next.
        if self.current_frame_len.is_none() {
            self.set_current_frame();
        }

        let item = self.input.next();
        if item.is_some() {
            self.samples_counted += 1;

            // At the end of a frame add the duration of this frame to
            // offset_duration and start collecting samples again.
            if Some(self.samples_counted) == self.current_frame_len() {
                self.offset_duration += self.samples_counted as f64
                    / self.current_frame_sample_rate as f64
                    / self.current_frame_channels as f64;

                // Reset.
                self.samples_counted = 0;
                self.set_current_frame();
            };
        };
        self.position.store(self.get_pos(), Ordering::Relaxed);
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
            self.offset_duration = pos.as_secs_f64();
            // This assumes that the seek implementation of the codec always
            // starts again at the beginning of a frame. Which is the case with
            // symphonia.
            self.samples_counted = 0;
            self.position.store(self.get_pos(), Ordering::Relaxed);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::buffer::SamplesBuffer;
    use crate::sink::AtomicF64;
    use crate::source::Source;

    #[test]
    fn test_position() {
        let inner = SamplesBuffer::new(1, 1, vec![10i16, -10, 10, -10, 20, -20]);
        let position = Arc::new(AtomicF64::new(0.0));
        let mut source = inner.trackable(position.clone());

        assert_eq!(position.load(Ordering::Relaxed), 0.0);
        source.next();
        assert_eq!(position.load(Ordering::Relaxed), 1.0);

        source.next();
        assert_eq!(position.load(Ordering::Relaxed), 2.0);

        assert_eq!(source.try_seek(Duration::new(1, 0)).is_ok(), true);
        assert_eq!(position.load(Ordering::Relaxed), 1.0);
    }
}
