use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::Source;

/// Tracks the elapsed duration since the start of the underlying source.
pub struct TrackPosition<I> {
    input: I,
    samples_counted: usize,
    offset_duration: f64,
    current_span_sample_rate: SampleRate,
    current_span_channels: ChannelCount,
}

impl<I> std::fmt::Debug for TrackPosition<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackPosition")
            .field("samples_counted", &self.samples_counted)
            .field("offset_duration", &self.offset_duration)
            .field("current_span_sample_rate", &self.current_span_sample_rate)
            .field("current_span_channels", &self.current_span_channels)
            .finish()
    }
}

impl<I: Source> TrackPosition<I> {
    pub(crate) fn new(source: I) -> TrackPosition<I> {
        assert!(source.sample_rate() > 0);
        TrackPosition {
            samples_counted: 0,
            offset_duration: 0.0,
            current_span_sample_rate: source.sample_rate(),
            current_span_channels: source.channels(),
            input: source,
        }
    }

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
    /// together with [`Source::try_seek()`] as the latter does take all
    /// speedup's and delay's into account. Its recommended therefore to apply
    /// track_position after speedup's and delay's.
    #[inline]
    pub fn get_pos(&self) -> Duration {
        let seconds = self.samples_counted as f64
            / self.input.sample_rate() as f64
            / self.input.channels().get() as f64
            + self.offset_duration;
        Duration::from_secs_f64(seconds)
    }
}

impl<I> Iterator for TrackPosition<I>
where
    I: Source,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        let item = self.input.next();
        if item.is_some() {
            self.samples_counted += 1;

            // At the end of a span add the duration of this span to
            // offset_duration and start collecting samples again.
            if self.parameters_changed() {
                self.offset_duration += self.samples_counted as f64
                    / self.current_span_sample_rate as f64
                    / self.current_span_channels.get() as f64;

                // Reset.
                self.samples_counted = 0;
                self.current_span_sample_rate = self.sample_rate();
                self.current_span_channels = self.channels();
            };
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
{
    #[inline]
    fn parameters_changed(&self) -> bool {
        self.input.parameters_changed()
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
        let result = self.input.try_seek(pos);
        if result.is_ok() {
            self.offset_duration = pos.as_secs_f64();
            // This assumes that the seek implementation of the codec always
            // starts again at the beginning of a span. Which is the case with
            // symphonia.
            self.samples_counted = 0;
        }
        result
    }
}
