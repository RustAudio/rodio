use core::fmt;
use std::{thread, time::Duration};

use crate::common::assert_error_traits;
use crate::conversions::SampleTypeConverter;
use crate::{microphone::config::InputConfig, ChannelCount, Sample, SampleRate, Source};

/// Builder for microphone configuration.
pub mod builder;
mod config;
pub use builder::MicrophoneBuilder;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device,
};
use rtrb::RingBuffer;

/// Error that can occur when we can not list the input devices
#[derive(Debug, thiserror::Error, Clone)]
#[error("Could not list input devices")]
pub struct ListError(#[source] cpal::DevicesError);
assert_error_traits! {ListError}

/// An input device
pub struct Input {
    inner: cpal::Device,
}

impl From<Input> for cpal::Device {
    fn from(val: Input) -> Self {
        val.inner
    }
}

impl fmt::Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("inner", &self.inner.name().unwrap_or("unknown".to_string()))
            .finish()
    }
}

impl fmt::Display for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner.name().unwrap_or("unknown".to_string()))
    }
}

/// Returns a list of available input devices on the system.
pub fn available_inputs() -> Result<Vec<Input>, ListError> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(ListError)?
        .map(|dev| Input { inner: dev });
    Ok(devices.collect())
}

/// A microphone input stream that can be used as an audio source.
pub struct Microphone {
    _stream_handle: cpal::Stream,
    buffer: rtrb::Consumer<Sample>,
    channels: ChannelCount,
    sample_rate: SampleRate,
    time_between_20_frames: Duration,
}

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
        loop {
            if let Ok(sample) = self.buffer.pop() {
                return Some(sample);
            } else {
                thread::sleep(self.time_between_20_frames)
            }
        }
    }
}

/// Errors that can occur when opening a microphone source.
#[derive(Debug, thiserror::Error, Clone)]
pub enum OpenError {
    /// Failed to build the input stream.
    #[error("Could not open microphone")]
    BuildStream(#[source] cpal::BuildStreamError),
    /// The sample format is not supported.
    #[error("This is a bug, please report it")]
    UnsupportedSampleFormat,
    /// Failed to start the input stream.
    #[error("Could not start the input stream")]
    Play(#[source] cpal::PlayStreamError),
}
assert_error_traits! {OpenError}

impl Microphone {
    fn open(
        device: Device,
        config: InputConfig,
        error_callback: impl FnMut(cpal::StreamError) + Send + 'static,
    ) -> Result<Self, OpenError> {
        let timeout = Some(Duration::from_millis(100));
        let (mut tx, rx) = RingBuffer::new(20_000);

        macro_rules! build_input_streams {
        ($($sample_format:tt, $generic:ty);+) => {
            match config.sample_format {
                $(
                    cpal::SampleFormat::$sample_format => device.build_input_stream::<$generic, _, _>(
                        &config.stream_config(),
                        move |data, _info| {
                            for sample in SampleTypeConverter::<_, f32>::new(data.into_iter().copied()) {
                                let _skip_if_player_is_behind = tx.push(sample);
                            }
                        },
                        error_callback,
                        timeout,
                    ),
                )+
                _ => return Err(OpenError::UnsupportedSampleFormat),
            }
        };
    }

        let stream = build_input_streams!(
            F32, f32;
            F64, f64;
            I8, i8;
            I16, i16;
            I32, i32;
            I64, i64;
            U8, u8;
            U16, u16;
            U32, u32;
            U64, u64
        )
        .map_err(OpenError::BuildStream)?;
        stream.play().map_err(OpenError::Play)?;

        Ok(Microphone {
            _stream_handle: stream,
            buffer: rx,
            channels: config.channel_count,
            sample_rate: config.sample_rate,
            time_between_20_frames: Duration::from_secs_f64(1.0 / config.sample_rate.get() as f64),
        })
    }
}
