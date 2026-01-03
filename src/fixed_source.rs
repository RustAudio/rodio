//! TODO
use std::time::Duration;

use crate::{ChannelCount, Sample, SampleRate};

/// TODO
pub trait FixedSource: Iterator<Item = Sample> {
    /// May NEVER return something else once its returned a value
    fn channels(&self) -> ChannelCount;
    /// May NEVER return something else once its returned a value
    fn sample_rate(&self) -> SampleRate;
    /// TODO
    fn total_duration(&self) -> Option<Duration>;
}
