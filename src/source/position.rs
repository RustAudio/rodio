use std::time::Duration;

use super::{SeekError, SpanTracker};
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
        span: SpanTracker::new(sample_rate, channels),
        input: source,
        offset_duration: 0.0,
    }
}

/// Tracks the elapsed duration since the start of the underlying source.
#[derive(Debug)]
pub struct TrackPosition<I> {
    input: I,
    offset_duration: Float,
    span: SpanTracker,
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
        let seconds = self.span.samples_counted as Float
            / self.input.sample_rate().get() as Float
            / self.input.channels().get() as Float
            + self.offset_duration;
        duration_from_secs(seconds)
    }
}

impl<I> Iterator for TrackPosition<I>
where
    I: Source,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        let item = self.input.next()?;

        // Capture state before advance() resets samples_counted at a boundary.
        let samples_before_boundary = self.span.samples_counted;
        let old_rate = self.span.last_sample_rate;
        let old_channels = self.span.last_channels;

        let detection = self.span.advance(&self.input);

        if detection.at_span_boundary {
            // Accumulate duration using the OLD parameters. advance() increments
            // samples_counted before resetting, so +1 gives the completed span count.
            let completed_samples = samples_before_boundary.saturating_add(1);
            self.offset_duration +=
                completed_samples as Float / old_rate.get() as Float / old_channels.get() as Float;
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
        self.span.seek(pos, &self.input);
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
