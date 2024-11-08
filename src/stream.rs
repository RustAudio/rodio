use std::{error, fmt};
use std::io::{Read, Seek};
use std::iter::empty;
use std::marker::Sync;
use std::sync::{Arc, Weak};

use cpal::{BufferSize, ChannelCount, FrameCount, PlayStreamError, Sample, SampleFormat, SampleRate, StreamConfig, SupportedBufferSize, SupportedStreamConfig};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::decoder;
use crate::dynamic_mixer::{mixer, DynamicMixer, DynamicMixerController};
use crate::sink::Sink;
use crate::source::Source;

const HZ_44100: cpal::SampleRate = cpal::SampleRate(44_100);

type SampleSource<S> = dyn Iterator<Item=S> + Send;

struct SourcePlug {
    source: Box<SampleSource<f32>>,
}

impl SourcePlug {
    pub fn new() -> Self {
        SourcePlug { source: Box::new(empty()) }
    }
}

impl Iterator for SourcePlug {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        self.source.next()
    }
}


/// `cpal::Stream` container. Also see the more useful `OutputStreamHandle`.
///
/// If this is dropped, playback will end & attached `OutputStreamHandle`s will no longer work.
pub struct OutputStream {
    mixer: Arc<DynamicMixerController<f32>>,
    _stream: cpal::Stream,
}

/// More flexible handle to a `OutputStream` that provides playback.
#[derive(Clone)]
pub struct OutputStreamHandle {
    mixer: Weak<DynamicMixerController<f32>>,
}

pub struct OutputHandle {
    stream: cpal::Stream,    
    mixer: Arc<DynamicMixerController<f32>>,
}

impl OutputHandle {
    
    pub fn play(&self) -> Result<(), PlayStreamError> {
        self.stream.play()
    }
}


#[derive(Copy, Clone, Debug)]
struct OutputStreamConfig {
    pub channel_count: ChannelCount,
    pub sample_rate: SampleRate,
    pub buffer_size: BufferSize,
    pub sample_format: SampleFormat,
}

#[derive(Default)]
struct OutputStreamBuilder {
    device: Option<cpal::Device>,
    config: OutputStreamConfig,
}

impl Default for OutputStreamConfig {
    fn default() -> Self {
        Self {
            channel_count: 2,
            sample_rate: HZ_44100,
            buffer_size: BufferSize::Default,
            sample_format: SampleFormat::I8,
        }
    }
}

impl OutputStreamBuilder {
    pub fn from_device(
        device: cpal::Device,
    ) -> Result<OutputStreamBuilder, StreamError> {
        let default_config = device.default_output_config()?;
        Ok(Self::default().with_supported_config(&default_config))
    }

    pub fn from_default_device() -> Result<OutputStreamBuilder, StreamError> {
        let default_device = cpal::default_host()
            .default_output_device()
            .ok_or(StreamError::NoDevice)?;
        Self::from_device(default_device)
    }

    pub fn with_device(mut self, device: cpal::Device) -> OutputStreamBuilder {
        self.device = Some(device);
        self
    }

    pub fn with_channels(mut self, channel_count: cpal::ChannelCount) -> OutputStreamBuilder {
        assert!(channel_count > 0);
        self.config.channel_count = channel_count;
        self
    }

    pub fn with_sample_rate(mut self, sample_rate: cpal::SampleRate) -> OutputStreamBuilder {
        self.config.sample_rate = sample_rate;
        self
    }

    pub fn with_buffer_size(mut self, buffer_size: cpal::BufferSize) -> OutputStreamBuilder {
        self.config.buffer_size = buffer_size;
        self
    }

    pub fn with_sample_format(mut self, sample_format: SampleFormat) -> OutputStreamBuilder {
        self.config.sample_format = sample_format;
        self
    }

    pub fn with_supported_config(mut self, config: &cpal::SupportedStreamConfig) -> OutputStreamBuilder {
        self.config = OutputStreamConfig {
            channel_count: config.channels(),
            sample_rate: config.sample_rate(),
            // FIXME See how to best handle buffer_size preferences
            buffer_size: clamp_supported_buffer_size(config.buffer_size(), 512),
            sample_format: config.sample_format(),
            ..self.config
        };
        self
    }

    pub fn with_config(mut self, config: &cpal::StreamConfig) -> OutputStreamBuilder {
        self.config = OutputStreamConfig {
            channel_count: config.channels,
            sample_rate: config.sample_rate,
            buffer_size: config.buffer_size,
            ..self.config
        };
        self
    }

    pub fn open_stream(&self) -> Result<OutputHandle, StreamError> {
        let device = self.device.as_ref().expect("output device specified");
        OutputHandle::open(device, &self.config)
    }

    /// FIXME Update documentation.
    /// Returns a new stream & handle using the given device and stream config.
    ///
    /// If the supplied `SupportedStreamConfig` is invalid for the device this function will
    /// fail to create an output stream and instead return a `StreamError`.
    pub fn try_open_stream(&self) -> Result<OutputHandle, StreamError> {
        let device = self.device.as_ref().expect("output device specified");
        OutputHandle::open(device, &self.config).or_else(|err| {
            for supported_config in supported_output_configs(device)? {
                if let Ok(handle) = Self::default().with_supported_config(&supported_config).open_stream() {
                    return Ok(handle);
                }
            }
            Err(err)
        })
    }

    /// FIXME Update docs
    ///
    /// Return a new stream & handle using the default output device.
    ///
    /// On failure will fall back to trying any non-default output devices.
    /// FIXME update the function.
    pub fn try_default_stream() -> Result<OutputHandle, StreamError> {
        let default_device = cpal::default_host()
            .default_output_device()
            .ok_or(StreamError::NoDevice)?;

        let default_stream = Self::from_device(default_device).and_then(|x| x.open_stream());

        default_stream.or_else(|original_err| {
            // default device didn't work, try other ones
            let mut devices = match cpal::default_host().output_devices() {
                Ok(d) => d,
                Err(_) => return Err(original_err),
            };

            devices
                .find_map(|d| Self::from_device(d).and_then(|x| x.open_stream()).ok())
                .ok_or(original_err)
        })
    }
}

fn clamp_supported_buffer_size(buffer_size: &SupportedBufferSize, preferred_size: FrameCount) -> BufferSize {
    todo!()
}

impl From<&OutputStreamConfig> for StreamConfig {
    fn from(config: &OutputStreamConfig) -> Self {
        cpal::StreamConfig {
            channels: config.channel_count,
            sample_rate: config.sample_rate,
            buffer_size: config.buffer_size,
        }
    }
}

// TODO (refactoring) Move necessary conveniences to the builder?
impl OutputStreamHandle {
    /// Plays a source with a device until it ends.
    pub fn play_raw<S>(&self, source: S) -> Result<(), PlayError>
    where
        S: Source<Item=f32> + Send + 'static,
    {
        let mixer = self.mixer.upgrade().ok_or(PlayError::NoDevice)?;
        mixer.add(source);
        Ok(())
    }

    /// Plays a sound once. Returns a `Sink` that can be used to control the sound.
    pub fn play_once<R>(&self, input: R) -> Result<Sink, PlayError>
    where
        R: Read + Seek + Send + Sync + 'static,
    {
        let input = decoder::Decoder::new(input)?;
        let sink = Sink::try_new(self)?;
        sink.append(input);
        Ok(sink)
    }
}

/// An error occurred while attempting to play a sound.
#[derive(Debug)]
pub enum PlayError {
    /// Attempting to decode the audio failed.
    DecoderError(decoder::DecoderError),
    /// The output device was lost.
    NoDevice,
}

impl From<decoder::DecoderError> for PlayError {
    fn from(err: decoder::DecoderError) -> Self {
        Self::DecoderError(err)
    }
}

impl fmt::Display for PlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DecoderError(e) => e.fmt(f),
            Self::NoDevice => write!(f, "NoDevice"),
        }
    }
}

impl error::Error for PlayError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::DecoderError(e) => Some(e),
            Self::NoDevice => None,
        }
    }
}

#[derive(Debug)]
pub enum StreamError {
    PlayStreamError(cpal::PlayStreamError),
    DefaultStreamConfigError(cpal::DefaultStreamConfigError),
    BuildStreamError(cpal::BuildStreamError),
    SupportedStreamConfigsError(cpal::SupportedStreamConfigsError),
    NoDevice,
}

impl From<cpal::DefaultStreamConfigError> for StreamError {
    fn from(err: cpal::DefaultStreamConfigError) -> Self {
        Self::DefaultStreamConfigError(err)
    }
}

impl From<cpal::SupportedStreamConfigsError> for StreamError {
    fn from(err: cpal::SupportedStreamConfigsError) -> Self {
        Self::SupportedStreamConfigsError(err)
    }
}

impl From<cpal::BuildStreamError> for StreamError {
    fn from(err: cpal::BuildStreamError) -> Self {
        Self::BuildStreamError(err)
    }
}

impl From<cpal::PlayStreamError> for StreamError {
    fn from(err: cpal::PlayStreamError) -> Self {
        Self::PlayStreamError(err)
    }
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::PlayStreamError(e) => e.fmt(f),
            Self::BuildStreamError(e) => e.fmt(f),
            Self::DefaultStreamConfigError(e) => e.fmt(f),
            Self::SupportedStreamConfigsError(e) => e.fmt(f),
            Self::NoDevice => write!(f, "NoDevice"),
        }
    }
}

impl error::Error for StreamError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::PlayStreamError(e) => Some(e),
            Self::BuildStreamError(e) => Some(e),
            Self::DefaultStreamConfigError(e) => Some(e),
            Self::SupportedStreamConfigsError(e) => Some(e),
            Self::NoDevice => None,
        }
    }
}

impl OutputHandle {
    pub fn open(device: &cpal::Device, config: &OutputStreamConfig) -> Result<OutputHandle, StreamError> {
        let (controller, source) = mixer(config.channel_count, config.sample_rate.0);
        Self::init_stream(device, config, source)
            .map_err(|x| StreamError::from(x))
            .and_then(|stream| {
                stream.play()?;
                Ok(Self { stream, mixer: controller })
            })
    }

    fn init_stream(
        device: &cpal::Device,
        config: &OutputStreamConfig,
        mut samples: DynamicMixer<f32>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError> {
        let error_callback = |err| eprintln!("an error occurred on output stream: {}", err);
        let sample_format = config.sample_format;
        let config = config.into();
        match sample_format {
            cpal::SampleFormat::F32 =>
                device.build_output_stream::<f32, _, _>(
                    &config,
                    move |data, _| {
                        data.iter_mut()
                            .for_each(|d| *d = samples.next().unwrap_or(0f32))
                    },
                    error_callback,
                    None,
                ),
            cpal::SampleFormat::F64 => device.build_output_stream::<f64, _, _>(
                &config,
                move |data, _| {
                    data.iter_mut()
                        .for_each(|d| *d = samples.next().map(Sample::from_sample).unwrap_or(0f64))
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::I8 => device.build_output_stream::<i8, _, _>(
                &config,
                move |data, _| {
                    data.iter_mut()
                        .for_each(|d| *d = samples.next().map(Sample::from_sample).unwrap_or(0i8))
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream::<i16, _, _>(
                &config,
                move |data, _| {
                    data.iter_mut()
                        .for_each(|d| *d = samples.next().map(Sample::from_sample).unwrap_or(0i16))
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::I32 => device.build_output_stream::<i32, _, _>(
                &config,
                move |data, _| {
                    data.iter_mut()
                        .for_each(|d| *d = samples.next().map(Sample::from_sample).unwrap_or(0i32))
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::I64 => device.build_output_stream::<i64, _, _>(
                &config,
                move |data, _| {
                    data.iter_mut()
                        .for_each(|d| *d = samples.next().map(Sample::from_sample).unwrap_or(0i64))
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::U8 => device.build_output_stream::<u8, _, _>(
                &config,
                move |data, _| {
                    data.iter_mut().for_each(|d| {
                        *d = samples
                            .next()
                            .map(Sample::from_sample)
                            .unwrap_or(u8::max_value() / 2)
                    })
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_output_stream::<u16, _, _>(
                &config,
                move |data, _| {
                    data.iter_mut().for_each(|d| {
                        *d = samples
                            .next()
                            .map(Sample::from_sample)
                            .unwrap_or(u16::max_value() / 2)
                    })
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::U32 => device.build_output_stream::<u32, _, _>(
                &config,
                move |data, _| {
                    data.iter_mut().for_each(|d| {
                        *d = samples
                            .next()
                            .map(Sample::from_sample)
                            .unwrap_or(u32::max_value() / 2)
                    })
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::U64 => device.build_output_stream::<u64, _, _>(
                &config,
                move |data, _| {
                    data.iter_mut().for_each(|d| {
                        *d = samples
                            .next()
                            .map(Sample::from_sample)
                            .unwrap_or(u64::max_value() / 2)
                    })
                },
                error_callback,
                None,
            ),
            _ => Err(cpal::BuildStreamError::StreamConfigNotSupported),
        }
    }
}

/// All the supported output formats with sample rates
fn supported_output_configs(
    device: &cpal::Device,
) -> Result<impl Iterator<Item=cpal::SupportedStreamConfig>, StreamError> {
    let mut supported: Vec<_> = device.supported_output_configs()?.collect();
    supported.sort_by(|a, b| b.cmp_default_heuristics(a));

    Ok(supported.into_iter().flat_map(|sf| {
        let max_rate = sf.max_sample_rate();
        let min_rate = sf.min_sample_rate();
        let mut formats = vec![sf.clone().with_max_sample_rate()];
        if HZ_44100 < max_rate && HZ_44100 > min_rate {
            formats.push(sf.clone().with_sample_rate(HZ_44100))
        }
        formats.push(sf.with_sample_rate(min_rate));
        formats
    }))
}
