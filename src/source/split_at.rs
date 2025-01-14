use std::ops::Range;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use crate::ChannelCount;
use crate::SampleRate;

use super::Source;
use super::TrackPosition;

pub struct Segment<S> {
    shared_source: Arc<Mutex<Option<TrackPosition<S>>>>,
    active: Option<TrackPosition<S>>,
    segment_range: Range<Duration>,
    split_duration: Option<Duration>,
}

impl<S> Segment<S>
where
    S: Source,
    <S as Iterator>::Item: crate::Sample,
{
    /// see docs at [Source::split_once];
    pub(crate) fn new(input: S, split_point: Duration) -> [Self; 2] {
        let shared_source = Arc::new(Mutex::new(None));
        let total_duration = input.total_duration();
        [
            Self {
                shared_source: shared_source.clone(),
                active: Some(input.track_position()),
                split_duration: Some(split_point),
                segment_range: Duration::ZERO..split_point,
            },
            Self {
                shared_source,
                active: None,
                split_duration: total_duration.map(|d| d.saturating_sub(split_point)),
                segment_range: split_point..Duration::MAX,
            },
        ]
    }

    fn deactivate(&mut self) {
        let Some(input) = self.active.take() else {
            return;
        };
        let mut shared = self
            .shared_source
            .lock()
            .expect("The audio thread can not panic while taking the shared source");
        *shared = Some(input);
    }
}

impl<S> Iterator for Segment<S>
where
    S: Source,
    S::Item: crate::Sample,
{
    type Item = <S as Iterator>::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let input = if let Some(active) = self.active.as_mut() {
            active
        } else {
            // did they other stop and is it in our segment?
            let mut shared = self
                .shared_source
                .lock()
                .expect("The audio thread cant panic deactivating");
            let input_pos = shared.as_mut()?.get_pos();
            if self.segment_range.contains(&input_pos) {
                self.active = shared.take();
                self.active.as_mut()?
            } else {
                return None;
            }
        };

        // There is some optimization potential here we are not using currently.
        // Calling get_pos once per span should be enough
        if input.get_pos() < self.segment_range.end {
            input.next()
        } else {
            self.deactivate();
            None
        }
    }
}

impl<S> Source for Segment<S>
where
    S: Source,
    S::Item: crate::Sample,
{
    fn current_span_len(&self) -> Option<usize> {
        if let Some(input) = self.active.as_ref() {
            input.current_span_len()
        } else {
            // We do not know the channel count nor sample rate if the source
            // is inactive. We will provide dummy values. This ensures the
            // caller will recheck when we become active
            Some(1)
        }
    }

    fn channels(&self) -> ChannelCount {
        self.active
            .as_ref()
            .map(Source::channels)
            .unwrap_or_default()
    }

    fn sample_rate(&self) -> SampleRate {
        self.active
            .as_ref()
            .map(Source::sample_rate)
            .unwrap_or_default()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.split_duration
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), super::SeekError> {
        if let Some(active) = self.active.as_mut() {
            active.try_seek(pos)?;
            if !self.segment_range.contains(&pos) {
                self.deactivate();
            }
            Ok(())
        } else {
            Err(super::SeekError::SegmentNotActive)
        }
    }
}
