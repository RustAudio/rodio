use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, Float, SampleRate};
use crate::math::{duration_from_secs, duration_to_float};
use crate::Source;

/// Internal function that builds a `TrackPosition` object. See trait docs for
/// details
pub fn track_position<I>(source: I) -> TrackPosition<I>
where
    I: Source,
{
    let channels = source.channels();
    let sample_rate = source.sample_rate();
    TrackPosition {
        input: source,
        samples_counted: 0,
        offset_duration: 0.0,
        current_span_sample_rate: sample_rate,
        current_span_channels: channels,
        current_span_len: None,
    }
}

/// Tracks the elapsed duration since the start of the underlying source.
#[derive(Debug)]
pub struct TrackPosition<I> {
    input: I,
    samples_counted: usize,
    offset_duration: Float,
    current_span_sample_rate: SampleRate,
    current_span_channels: ChannelCount,
    current_span_len: Option<usize>,
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
{
    /// Returns the position of the underlying source relative to its start.
    ///
    /// If a speedup and or delay is applied after applying a
    /// [`Source::track_position`] it will not be reflected in the position
    /// returned by [`get_pos`](TrackPosition::get_pos).
    ///
    /// This can get confusing when using [`get_pos()`](TrackPosition::get_pos)
    /// together with [`Source::try_seek()`] as the the latter does take all
    /// speedup's and delay's into account. Its recommended therefore to apply
    /// track_position after speedup's and delay's.
    #[inline]
    pub fn get_pos(&self) -> Duration {
        let seconds = self.samples_counted as Float
            / self.input.sample_rate().get() as Float
            / self.input.channels().get() as Float
            + self.offset_duration;
        duration_from_secs(seconds)
    }

    #[inline]
    fn reset_current_span(&mut self) {
        self.samples_counted = 0;
        self.current_span_len = self.current_span_len();
        self.current_span_sample_rate = self.sample_rate();
        self.current_span_channels = self.channels();
    }
}

impl<I> Iterator for TrackPosition<I>
where
    I: Source,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        if self.current_span_len.is_none() {
            self.current_span_len = self.input.current_span_len();
        }

        let item = self.input.next()?;
        self.samples_counted += 1;

        // Detect span boundaries by TWO mechanisms:
        // 1. Reached end of span (by length) - handles same-parameter consecutive spans
        // 2. Parameters changed - handles mid-stream parameter changes
        let new_channels = self.input.channels();
        let new_sample_rate = self.input.sample_rate();

        let length_boundary = self
            .current_span_len
            .is_some_and(|len| self.samples_counted >= len);
        let parameter_boundary = new_channels != self.current_span_channels
            || new_sample_rate != self.current_span_sample_rate;

        if length_boundary || parameter_boundary {
            // At span boundary - accumulate duration using OLD parameters
            self.offset_duration += self.samples_counted as Float
                / self.current_span_sample_rate.get() as Float
                / self.current_span_channels.get() as Float;
            self.reset_current_span();
        }

        Some(item)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for TrackPosition<I> where I: Source + ExactSizeIterator {}

impl<I> Source for TrackPosition<I>
where
    I: Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.input.channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.input.try_seek(pos)?;
        self.offset_duration = duration_to_float(pos);
        // Set current_span_len to None after seeking because we may have landed mid-span.
        // We don't know how many samples remain in the current span, so we disable
        // length-based boundary detection until the next parameter change, which will
        // put us at the start of a fresh span where we can re-enable it.
        self.reset_current_span();
        self.current_span_len = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::buffer::SamplesBuffer;
    use crate::math::nz;
    use crate::source::Source;

    #[test]
    fn test_position() {
        let inner = SamplesBuffer::new(nz!(1), nz!(1), vec![10.0, -10.0, 10.0, -10.0, 20.0, -20.0]);
        let mut source = inner.track_position();

        assert_eq!(source.get_pos().as_secs_f32(), 0.0);
        source.next();
        assert_eq!(source.get_pos().as_secs_f32(), 1.0);

        source.next();
        assert_eq!(source.get_pos().as_secs_f32(), 2.0);

        assert!(source.try_seek(Duration::new(1, 0)).is_ok());
        assert_eq!(source.get_pos().as_secs_f32(), 1.0);
    }

    #[test]
    fn test_position_in_presence_of_speedup() {
        let inner = SamplesBuffer::new(nz!(1), nz!(1), vec![10.0, -10.0, 10.0, -10.0, 20.0, -20.0]);
        let mut source = inner.speed(2.0).track_position();

        assert_eq!(source.get_pos().as_secs_f32(), 0.0);
        source.next();
        assert_eq!(source.get_pos().as_secs_f32(), 0.5);

        source.next();
        assert_eq!(source.get_pos().as_secs_f32(), 1.0);

        assert!(source.try_seek(Duration::new(1, 0)).is_ok());
        assert_eq!(source.get_pos().as_secs_f32(), 1.0);
    }
}
