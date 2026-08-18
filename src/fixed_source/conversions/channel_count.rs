use std::time::Duration;

use crate::source::SeekError;
use crate::{ChannelCount, Sample};
use crate::{FixedSource, SampleRate};

/// Converts between two channel counts, created using
/// [with_channel_count](FixedSource::with_channel_count)
pub struct ChannelConverter<S> {
    input: S,
    pub(crate) target: ChannelCount,
    sample_repeat: Option<Sample>,
    next_output_sample_pos: u16,
}

impl<S: FixedSource> ChannelConverter<S> {
    pub(crate) fn new(input: S, target: ChannelCount) -> Self {
        Self {
            input,
            target,
            sample_repeat: None,
            next_output_sample_pos: 0,
        }
    }

    crate::common::source::add_inner_accessors! {input}
}

impl<S: FixedSource> FixedSource for ChannelConverter<S> {
    fn channels(&self) -> ChannelCount {
        self.target
    }

    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        self.input.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.input.try_seek(pos)
    }
}

// TODO optimize (still assumes dynamicsource)
impl<S: FixedSource> Iterator for ChannelConverter<S> {
    type Item = crate::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let result = match self.next_output_sample_pos {
            0 => {
                // save first sample for mono -> stereo conversion
                let value = self.input.next();
                self.sample_repeat = value;
                value
            }
            x if x < self.input.channels().get() => {
                // make sure we always end on a frame boundary
                let value = self.input.next();
                assert!(value.is_some(), "Sources may not emit half frames");
                value
            }
            1 => self.sample_repeat,
            _ => Some(0.0), // all other added channels are empty
        };

        if result.is_some() {
            self.next_output_sample_pos += 1;
        }

        if self.next_output_sample_pos == self.target.get() {
            self.next_output_sample_pos = 0;

            if self.input.channels() > self.target {
                for _ in self.target.get()..self.input.channels().get() {
                    self.input.next(); // discarding extra input
                }
            }
        }
        result
    }
}
