//! Output audio via the OS via mixers or play directly
//!
//! This module provides a builder that's used to configure and open audio output. Once
//! opened sources can be mixed into the output via `OutputStream::mixer`.
//!
//! There is also a convenience function `play` for using that output mixer to
//! play a single sound.
use crate::common::{ChannelCount, SampleRate};
use crate::decoder;
use crate::mixer::{mixer, Mixer, MixerSource};
use crate::sink::Sink;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Sample, SampleFormat, StreamConfig};
use std::io::{Read, Seek};
use std::marker::Sync;
use std::{error, fmt};

const HZ_44100: SampleRate = 44_100;

/// `cpal::Stream` container.
/// Use `mixer()` method to control output.
/// If this is dropped, playback will end, and the associated output stream will be disposed.
pub struct OutputStream {
    config: OutputStreamConfig,
    mixer: Mixer,
    _stream: cpal::Stream,
}

impl OutputStream {
    /// Access the output stream's mixer.
    pub fn mixer(&self) -> &Mixer {
        &self.mixer
    }

    /// Access the output stream's config.
    pub fn config(&self) -> &OutputStreamConfig {
        &self.config
    }
}

/// Describes the output stream's configuration
#[derive(Copy, Clone, Debug)]
pub struct OutputStreamConfig {
    channel_count: ChannelCount,
    sample_rate: SampleRate,
    buffer_size: BufferSize,
    sample_format: SampleFormat,
}

impl Default for OutputStreamConfig {
    fn default() -> Self {
        Self {
            channel_count: 2,
            sample_rate: HZ_44100,
            buffer_size: BufferSize::Default,
            sample_format: SampleFormat::F32,
        }
    }
}

impl OutputStreamConfig {
    /// Access the output stream config's channel count.
    pub fn channel_count(&self) -> ChannelCount {
        self.channel_count
    }

    /// Access the output stream config's sample rate.
    pub fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    /// Access the output stream config's buffer size.
    pub fn buffer_size(&self) -> &BufferSize {
        &self.buffer_size
    }

    /// Access the output stream config's sample format.
    pub fn sample_format(&self) -> SampleFormat {
        self.sample_format
    }
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

fn default_error_callback(err: cpal::StreamError) {
    #[cfg(feature = "tracing")]
    tracing::error!("audio stream error: {err}");
    #[cfg(not(feature = "tracing"))]
    eprintln!("audio stream error: {err}");
}

/// Convenience builder for audio output stream.
/// It provides methods to configure several parameters of the audio output and opening default
/// device. See examples for use-cases.
pub struct OutputStreamBuilder<E = fn(cpal::StreamError)>
where
    E: FnMut(cpal::StreamError) + Send + 'static,
{
    device: Option<cpal::Device>,
    config: OutputStreamConfig,
    error_callback: E,
}

impl Default for OutputStreamBuilder {
    fn default() -> Self {
        Self {
            device: None,
            config: OutputStreamConfig::default(),
            error_callback: default_error_callback,
        }
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

impl<E> OutputStreamBuilder<E>
where
    E: FnMut(cpal::StreamError) + Send + 'static,
{
    /// Sets output audio device keeping all existing stream parameters intact.
    /// This method is useful if you want to set other parameters yourself.
    /// To also set parameters that are appropriate for the device use [Self::from_device()] instead.
    pub fn with_device(mut self, device: cpal::Device) -> OutputStreamBuilder<E> {
        self.device = Some(device);
        self
    }

    /// Sets number of output stream's channels.
    pub fn with_channels(mut self, channel_count: ChannelCount) -> OutputStreamBuilder<E> {
        assert!(channel_count > 0);
        self.config.channel_count = channel_count;
        self
    }

    /// Sets output stream's sample rate.
    pub fn with_sample_rate(mut self, sample_rate: SampleRate) -> OutputStreamBuilder<E> {
        self.config.sample_rate = sample_rate;
        self
    }

    /// Sets preferred output buffer size.
    ///
    /// To play sound without any glitches the audio card may never receive a
    /// sample to late. Some samples might take longer to generate then
    /// others. For example because:
    ///  - The OS preempts the thread creating the samples. This happens more
    ///    often if the computer is under high load.
    ///  - The decoder needs to read more data from disk.
    ///  - Rodio code takes longer to run for some samples then others
    ///  - The OS can only send audio samples in groups to the DAC.
    ///
    /// The OS solves this by buffering samples. The larger that buffer the
    /// smaller the impact of variable sample generation time. On the other
    /// hand Rodio controls audio by changing the value of samples. We can not
    /// change a sample already in the OS buffer. That means there is a
    /// minimum delay (latency) of `<buffer size>/<sample_rate*channel_count>`
    /// seconds before a change made through rodio takes effect.
    ///
    /// # Large vs Small buffer
    /// - A larger buffer size results in high latency. Changes made trough
    ///   Rodio (volume/skip/effects etc) takes longer before they can be heard.
    /// - A small buffer might cause:
    ///   - Higher CPU usage
    ///   - Playback interruptions such as buffer underruns.
    ///   - Rodio to log errors like: `alsa::poll() returned POLLERR`
    ///
    /// # Recommendation
    /// If low latency is important to you consider offering the user a method
    /// to find the minimum buffer size that works well on their system under
    /// expected conditions. A good example of this approach can be seen in
    /// [mumble](https://www.mumble.info/documentation/user/audio-settings/)
    /// (specifically the *Output Delay* & *Jitter buffer*.
    ///
    /// These are some typical values that are a good starting point. They may also
    /// break audio completely, it depends on the system.
    /// - Low-latency (audio production, live monitoring): 512-1024
    /// - General use (games, media playback): 1024-2048
    /// - Stability-focused (background music, non-interactive): 2048-4096
    pub fn with_buffer_size(mut self, buffer_size: cpal::BufferSize) -> OutputStreamBuilder<E> {
        self.config.buffer_size = buffer_size;
        self
    }

    /// Select scalar type that will carry a sample.
    pub fn with_sample_format(mut self, sample_format: SampleFormat) -> OutputStreamBuilder<E> {
        self.config.sample_format = sample_format;
        self
    }

    /// Set available parameters from a CPAL supported config. You can get a list of
    /// such configurations for an output device using [crate::stream::supported_output_configs()]
    pub fn with_supported_config(
        mut self,
        config: &cpal::SupportedStreamConfig,
    ) -> OutputStreamBuilder<E> {
        self.config = OutputStreamConfig {
            channel_count: config.channels() as ChannelCount,
            sample_rate: config.sample_rate().0 as SampleRate,
            sample_format: config.sample_format(),
            ..Default::default()
        };
        self
    }

    /// Set all output stream parameters at once from CPAL stream config.
    pub fn with_config(mut self, config: &cpal::StreamConfig) -> OutputStreamBuilder<E> {
        self.config = OutputStreamConfig {
            channel_count: config.channels as ChannelCount,
            sample_rate: config.sample_rate.0 as SampleRate,
            buffer_size: config.buffer_size,
            ..self.config
        };
        self
    }

    /// Set a callback that will be called when an error occurs with the stream
    pub fn with_error_callback<F>(self, callback: F) -> OutputStreamBuilder<F>
    where
        F: FnMut(cpal::StreamError) + Send + 'static,
    {
        OutputStreamBuilder {
            device: self.device,
            config: self.config,
            error_callback: callback,
        }
    }

    /// Open output stream using parameters configured so far.
    pub fn open_stream(self) -> Result<OutputStream, StreamError> {
        let device = self.device.as_ref().expect("output device specified");

        OutputStream::open(device, &self.config, self.error_callback)
    }

    /// Try opening a new output stream with the builder's current stream configuration.
    /// Failing that attempt to open stream with other available configurations
    /// supported by the device.
    /// If all attempts fail returns initial error.
    pub fn open_stream_or_fallback(&self) -> Result<OutputStream, StreamError>
    where
        E: Clone,
    {
        let device = self.device.as_ref().expect("output device specified");
        let error_callback = &self.error_callback;

        OutputStream::open(device, &self.config, error_callback.clone()).or_else(|err| {
            for supported_config in supported_output_configs(device)? {
                if let Ok(handle) = OutputStreamBuilder::default()
                    .with_device(device.clone())
                    .with_supported_config(&supported_config)
                    .with_error_callback(error_callback.clone())
                    .open_stream()
                {
                    return Ok(handle);
                }
            }
            Err(err)
        })
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
            channels: config.channel_count as cpal::ChannelCount,
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
        assert!(
            config.channel_count > 0,
            "channel number is greater than zero"
        );
    }

    fn open<E>(
        device: &cpal::Device,
        config: &OutputStreamConfig,
        error_callback: E,
    ) -> Result<OutputStream, StreamError>
    where
        E: FnMut(cpal::StreamError) + Send + 'static,
    {
        Self::validate_config(config);
        let (controller, source) = mixer(config.channel_count, config.sample_rate);
        Self::init_stream(device, config, source, error_callback).and_then(|stream| {
            stream.play().map_err(StreamError::PlayStreamError)?;
            Ok(Self {
                _stream: stream,
                mixer: controller,
                config: *config,
            })
        })
    }

    fn init_stream<E>(
        device: &cpal::Device,
        config: &OutputStreamConfig,
        mut samples: MixerSource,
        error_callback: E,
    ) -> Result<cpal::Stream, StreamError>
    where
        E: FnMut(cpal::StreamError) + Send + 'static,
    {
        let sample_format = config.sample_format;
        let config = config.into();

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
pub fn supported_output_configs(
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
