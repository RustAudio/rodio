use crate::common::{ChannelCount, SampleRate};
use crate::decoder;
use crate::math::ch;
use crate::mixer::{mixer, Mixer, MixerSource};
use crate::sink::Sink;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FrameCount, Sample, SampleFormat, StreamConfig, SupportedBufferSize};
use std::io::{Read, Seek};
use std::marker::Sync;
use std::sync::Arc;
use std::{error, fmt};

const HZ_44100: SampleRate = 44_100;

/// `cpal::Stream` container.
/// Use `mixer()` method to control output.
/// If this is dropped, playback will end, and the associated output stream will be disposed.
pub struct OutputStream {
    mixer: Arc<Mixer>,
    _stream: cpal::Stream,
}

impl OutputStream {
    /// Access the output stream's mixer.
    pub fn mixer(&self) -> Arc<Mixer> {
        self.mixer.clone()
    }
}

#[derive(Copy, Clone, Debug)]
struct OutputStreamConfig {
    channel_count: ChannelCount,
    sample_rate: SampleRate,
    buffer_size: BufferSize,
    sample_format: SampleFormat,
}

impl Default for OutputStreamConfig {
    fn default() -> Self {
        Self {
            channel_count: ch!(2),
            sample_rate: HZ_44100,
            buffer_size: BufferSize::Default,
            sample_format: SampleFormat::F32,
        }
    }
}

/// Convenience builder for audio output stream.
/// It provides methods to configure several parameters of the audio output and opening default
/// device. See examples for use-cases.
#[derive(Default)]
pub struct OutputStreamBuilder {
    device: Option<cpal::Device>,
    config: OutputStreamConfig,
}

impl core::fmt::Debug for OutputStreamBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let device = if let Some(device) = &self.device {
            "Some(".to_owned() + device.name().as_deref().unwrap_or("UnNamed") + ")"
        } else {
            "None".to_owned()
        };

        f.debug_struct("OutputStreamBuilder")
            .field("device", &device)
            .field("config", &self.config)
            .finish()
    }
}

impl OutputStreamBuilder {
    /// Sets output device and its default parameters.
    pub fn from_device(device: cpal::Device) -> Result<OutputStreamBuilder, StreamError> {
        let default_config = device
            .default_output_config()
            .map_err(StreamError::DefaultStreamConfigError)?;
        Ok(Self::default()
            .with_device(device)
            .with_supported_config(&default_config))
    }

    /// Sets default output stream parameters for default output audio device.
    pub fn from_default_device() -> Result<OutputStreamBuilder, StreamError> {
        let default_device = cpal::default_host()
            .default_output_device()
            .ok_or(StreamError::NoDevice)?;
        Self::from_device(default_device)
    }

    /// Sets output audio device keeping all existing stream parameters intact.
    /// This method is useful if you want to set other parameters yourself.
    /// To also set parameters that are appropriate for the device use [Self::from_device()] instead.
    pub fn with_device(mut self, device: cpal::Device) -> OutputStreamBuilder {
        self.device = Some(device);
        self
    }

    /// Sets number of output stream's channels.
    pub fn with_channels(mut self, channel_count: ChannelCount) -> OutputStreamBuilder {
        self.config.channel_count = channel_count;
        self
    }

    /// Sets output stream's sample rate.
    pub fn with_sample_rate(mut self, sample_rate: SampleRate) -> OutputStreamBuilder {
        self.config.sample_rate = sample_rate;
        self
    }

    /// Sets preferred output buffer size.
    /// Larger buffer size causes longer playback delays. Buffer sizes that are too small
    /// may cause higher CPU usage or playback interruptions.
    pub fn with_buffer_size(mut self, buffer_size: cpal::BufferSize) -> OutputStreamBuilder {
        self.config.buffer_size = buffer_size;
        self
    }

    /// Select scalar type that will carry a sample.
    pub fn with_sample_format(mut self, sample_format: SampleFormat) -> OutputStreamBuilder {
        self.config.sample_format = sample_format;
        self
    }

    /// Set available parameters from a CPAL supported config. You can get list of
    /// such configurations for an output device using [crate::stream::supported_output_configs()]
    pub fn with_supported_config(
        mut self,
        config: &cpal::SupportedStreamConfig,
    ) -> OutputStreamBuilder {
        self.config = OutputStreamConfig {
            channel_count: ChannelCount::new(config.channels())
                .expect("cpal should never return a zero channel output"),
            sample_rate: config.sample_rate().0 as SampleRate,
            // In case of supported range limit buffer size to avoid unexpectedly long playback delays.
            buffer_size: clamp_supported_buffer_size(config.buffer_size(), 1024),
            sample_format: config.sample_format(),
        };
        self
    }

    /// Set all output stream parameters at once from CPAL stream config.
    pub fn with_config(mut self, config: &cpal::StreamConfig) -> OutputStreamBuilder {
        self.config = OutputStreamConfig {
            channel_count: ChannelCount::new(config.channels)
                .expect("cpal should never return a zero channel output"),
            sample_rate: config.sample_rate.0 as SampleRate,
            buffer_size: config.buffer_size,
            ..self.config
        };
        self
    }

    /// Open output stream using parameters configured so far.
    pub fn open_stream(&self) -> Result<OutputStream, StreamError> {
        let device = self.device.as_ref().expect("output device specified");
        OutputStream::open(device, &self.config)
    }

    /// Try opening a new output stream with the builder's current stream configuration.
    /// Failing that attempt to open stream with other available configurations
    /// supported by the device.
    /// If all attempts fail returns initial error.
    pub fn open_stream_or_fallback(&self) -> Result<OutputStream, StreamError> {
        let device = self.device.as_ref().expect("output device specified");
        OutputStream::open(device, &self.config).or_else(|err| {
            for supported_config in supported_output_configs(device)? {
                if let Ok(handle) = Self::default()
                    .with_device(device.clone())
                    .with_supported_config(&supported_config)
                    .open_stream()
                {
                    return Ok(handle);
                }
            }
            Err(err)
        })
    }

    /// Try to open a new output stream for the default output device with its default configuration.
    /// Failing that attempt to open output stream with alternative configuration and/or non default
    /// output devices. Returns stream for first of the tried configurations that succeeds.
    /// If all attempts fail return the initial error.
    pub fn open_default_stream() -> Result<OutputStream, StreamError> {
        Self::from_default_device()
            .and_then(|x| x.open_stream())
            .or_else(|original_err| {
                let mut devices = match cpal::default_host().output_devices() {
                    Ok(devices) => devices,
                    Err(err) => {
                        #[cfg(feature = "tracing")]
                        tracing::error!("error getting list of output devices: {err}");
                        #[cfg(not(feature = "tracing"))]
                        eprintln!("error getting list of output devices: {err}");
                        return Err(original_err);
                    }
                };
                devices
                    .find_map(|d| {
                        Self::from_device(d)
                            .and_then(|x| x.open_stream_or_fallback())
                            .ok()
                    })
                    .ok_or(original_err)
            })
    }
}

fn clamp_supported_buffer_size(
    buffer_size: &SupportedBufferSize,
    preferred_size: FrameCount,
) -> BufferSize {
    match buffer_size {
        SupportedBufferSize::Range { min, max } => {
            let size = preferred_size.clamp(*min, *max);
            assert!(size > 0, "selected buffer size is greater than zero");
            BufferSize::Fixed(size)
        }
        SupportedBufferSize::Unknown => BufferSize::Default,
    }
}

/// A convenience function. Plays a sound once.
/// Returns a `Sink` that can be used to control the sound.
pub fn play<R>(mixer: &Mixer, input: R) -> Result<Sink, PlayError>
where
    R: Read + Seek + Send + Sync + 'static,
{
    let input = decoder::Decoder::new(input)?;
    let sink = Sink::connect_new(mixer);
    sink.append(input);
    Ok(sink)
}

impl From<&OutputStreamConfig> for StreamConfig {
    fn from(config: &OutputStreamConfig) -> Self {
        cpal::StreamConfig {
            channels: config.channel_count.get() as cpal::ChannelCount,
            sample_rate: cpal::SampleRate(config.sample_rate),
            buffer_size: config.buffer_size,
        }
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

/// Errors that might occur when interfacing with audio output.
#[derive(Debug)]
pub enum StreamError {
    /// Could not start playing the stream, see [cpal::PlayStreamError] for
    /// details.
    PlayStreamError(cpal::PlayStreamError),
    /// Failed to get the stream config for the given device. See
    /// [cpal::DefaultStreamConfigError] for details.
    DefaultStreamConfigError(cpal::DefaultStreamConfigError),
    /// Error opening stream with OS. See [cpal::BuildStreamError] for details.
    BuildStreamError(cpal::BuildStreamError),
    /// Could not list supported stream configs for the device. Maybe it
    /// disconnected. For details see: [cpal::SupportedStreamConfigsError].
    SupportedStreamConfigsError(cpal::SupportedStreamConfigsError),
    /// Could not find any output device
    NoDevice,
    /// New cpal sample format that rodio does not yet support please open
    /// an issue if you run into this.
    UnsupportedSampleFormat,
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::PlayStreamError(e) => e.fmt(f),
            Self::BuildStreamError(e) => e.fmt(f),
            Self::DefaultStreamConfigError(e) => e.fmt(f),
            Self::SupportedStreamConfigsError(e) => e.fmt(f),
            Self::NoDevice => write!(f, "NoDevice"),
            Self::UnsupportedSampleFormat => write!(f, "UnsupportedSampleFormat"),
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
            Self::UnsupportedSampleFormat => None,
        }
    }
}

impl OutputStream {
    fn validate_config(config: &OutputStreamConfig) {
        if let BufferSize::Fixed(sz) = config.buffer_size {
            assert!(sz > 0, "fixed buffer size is greater than zero");
        }
        assert!(config.sample_rate > 0, "sample rate is greater than zero");
    }

    fn open(
        device: &cpal::Device,
        config: &OutputStreamConfig,
    ) -> Result<OutputStream, StreamError> {
        Self::validate_config(config);
        let (controller, source) = mixer(config.channel_count, config.sample_rate);
        Self::init_stream(device, config, source).and_then(|stream| {
            stream.play().map_err(StreamError::PlayStreamError)?;
            Ok(Self {
                _stream: stream,
                mixer: controller,
            })
        })
    }

    fn init_stream(
        device: &cpal::Device,
        config: &OutputStreamConfig,
        mut samples: MixerSource,
    ) -> Result<cpal::Stream, StreamError> {
        let error_callback = |err| {
            #[cfg(feature = "tracing")]
            tracing::error!("Playback error: {err}");
            #[cfg(not(feature = "tracing"))]
            eprintln!("Playback error: {err}");
        };
        let sample_format = config.sample_format;
        let config: cpal::StreamConfig = config.into();
        match sample_format {
            cpal::SampleFormat::F32 => device.build_output_stream::<f32, _, _>(
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
                            .unwrap_or(u8::MAX / 2)
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
                            .unwrap_or(u16::MAX / 2)
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
                            .unwrap_or(u32::MAX / 2)
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
                            .unwrap_or(u64::MAX / 2)
                    })
                },
                error_callback,
                None,
            ),
            _ => return Err(StreamError::UnsupportedSampleFormat),
        }
        .map_err(StreamError::BuildStreamError)
    }
}

/// Return all formats supported by the device.
fn supported_output_configs(
    device: &cpal::Device,
) -> Result<impl Iterator<Item = cpal::SupportedStreamConfig>, StreamError> {
    let mut supported: Vec<_> = device
        .supported_output_configs()
        .map_err(StreamError::SupportedStreamConfigsError)?
        .collect();
    supported.sort_by(|a, b| b.cmp_default_heuristics(a));

    Ok(supported.into_iter().flat_map(|sf| {
        let max_rate = sf.max_sample_rate();
        let min_rate = sf.min_sample_rate();
        let mut formats = vec![sf.with_max_sample_rate()];
        let preferred_rate = cpal::SampleRate(HZ_44100);
        if preferred_rate < max_rate && preferred_rate > min_rate {
            formats.push(sf.with_sample_rate(preferred_rate))
        }
        formats.push(sf.with_sample_rate(min_rate));
        formats
    }))
}
