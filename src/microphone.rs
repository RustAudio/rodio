use crate::{ChannelCount, Sample, SampleRate, Source};

pub mod builder;
mod config;
pub use builder::MicrophoneBuilder;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("")]
    DefaultInputConfig(cpal::DefaultStreamConfigError),
}

pub struct Microphone {
    stream_handle: cpal::Stream,
    buffer: rtrb::Consumer<Sample>,
    channels: ChannelCount,
    sample_rate: SampleRate,
}

impl Microphone {}

impl Source for Microphone {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> crate::ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> crate::SampleRate {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

impl Iterator for Microphone {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        // In case of an underrun start playing silence
        // best we can do (will lead to loud pops)
        Some(self.buffer.pop().unwrap_or(0.0))
    }
}
