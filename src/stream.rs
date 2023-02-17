use std::io::{Read, Seek};
use std::marker::Sync;
use std::sync::{Arc, Weak};
use std::{error, fmt};

use crate::decoder;
use crate::dynamic_mixer::{self, DynamicMixerController};
use crate::sink::Sink;
use crate::source::Source;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SupportedStreamConfig};

/// `cpal::Stream` container. Also see the more useful `OutputStreamHandle`.
///
/// If this is dropped playback will end & attached `OutputStreamHandle`s will no longer work.
pub struct OutputStream {
    mixer: Arc<DynamicMixerController<f32>>,
    _stream: cpal::Stream,
}

/// More flexible handle to a `OutputStream` that provides playback.
#[derive(Clone)]
pub struct OutputStreamHandle {
    mixer: Weak<DynamicMixerController<f32>>,
}

impl OutputStream {
    /// Returns a new stream & handle using the given output device and the default output
    /// configuration.
    pub fn try_from_device(
        device: &cpal::Device,
    ) -> Result<(Self, OutputStreamHandle), StreamError> {
        let default_config = device.default_output_config()?;
        OutputStream::try_from_device_config(device, default_config)
    }

    /// Returns a new stream & handle using the given device and stream config.
    ///
    /// If the supplied `SupportedStreamConfig` is invalid for the device this function will
    /// fail to create an output stream and instead return a `StreamError`
    pub fn try_from_device_config(
        device: &cpal::Device,
        config: SupportedStreamConfig,
    ) -> Result<(Self, OutputStreamHandle), StreamError> {
        let (mixer, _stream) = device.try_new_output_stream_config(config)?;
        _stream.play()?;
        let out = Self { mixer, _stream };
        let handle = OutputStreamHandle {
            mixer: Arc::downgrade(&out.mixer),
        };
        Ok((out, handle))
    }

    /// Return a new stream & handle using the default output device.
    ///
    /// On failure will fallback to trying any non-default output devices.
    pub fn try_default() -> Result<(Self, OutputStreamHandle), StreamError> {
        let default_device = cpal::default_host()
            .default_output_device()
            .ok_or(StreamError::NoDevice)?;

        let default_stream = Self::try_from_device(&default_device);

        default_stream.or_else(|original_err| {
            // default device didn't work, try other ones
            let mut devices = match cpal::default_host().output_devices() {
                Ok(d) => d,
                Err(_) => return Err(original_err),
            };

            devices
                .find_map(|d| Self::try_from_device(&d).ok())
                .ok_or(original_err)
        })
    }
}

impl OutputStreamHandle {
    /// Plays a source with a device until it ends.
    pub fn play_raw<S>(&self, source: S) -> Result<(), PlayError>
    where
        S: Source<Item = f32> + Send + 'static,
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

/// Extensions to `cpal::Device`
pub(crate) trait CpalDeviceExt {
    fn new_output_stream_with_format(
        &self,
        format: cpal::SupportedStreamConfig,
    ) -> Result<(Arc<DynamicMixerController<f32>>, cpal::Stream), cpal::BuildStreamError>;

    fn try_new_output_stream_config(
        &self,
        config: cpal::SupportedStreamConfig,
    ) -> Result<(Arc<DynamicMixerController<f32>>, cpal::Stream), StreamError>;
}

impl CpalDeviceExt for cpal::Device {
    fn new_output_stream_with_format(
        &self,
        format: cpal::SupportedStreamConfig,
    ) -> Result<(Arc<DynamicMixerController<f32>>, cpal::Stream), cpal::BuildStreamError> {
        let (mixer_tx, mut mixer_rx) =
            dynamic_mixer::mixer::<f32>(format.channels(), format.sample_rate().0);

        let error_callback = |err| eprintln!("an error occurred on output stream: {}", err);

        match format.sample_format() {
            cpal::SampleFormat::F32 => self.build_output_stream::<f32, _, _>(
                &format.config(),
                move |data, _| {
                    data.iter_mut()
                        .for_each(|d| *d = mixer_rx.next().unwrap_or(0f32))
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::F64 => self.build_output_stream::<f64, _, _>(
                &format.config(),
                move |data, _| {
                    data.iter_mut()
                        .for_each(|d| *d = mixer_rx.next().map(Sample::from_sample).unwrap_or(0f64))
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::I8 => self.build_output_stream::<i8, _, _>(
                &format.config(),
                move |data, _| {
                    data.iter_mut()
                        .for_each(|d| *d = mixer_rx.next().map(Sample::from_sample).unwrap_or(0i8))
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::I16 => self.build_output_stream::<i16, _, _>(
                &format.config(),
                move |data, _| {
                    data.iter_mut()
                        .for_each(|d| *d = mixer_rx.next().map(Sample::from_sample).unwrap_or(0i16))
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::I32 => self.build_output_stream::<i32, _, _>(
                &format.config(),
                move |data, _| {
                    data.iter_mut()
                        .for_each(|d| *d = mixer_rx.next().map(Sample::from_sample).unwrap_or(0i32))
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::I64 => self.build_output_stream::<i64, _, _>(
                &format.config(),
                move |data, _| {
                    data.iter_mut()
                        .for_each(|d| *d = mixer_rx.next().map(Sample::from_sample).unwrap_or(0i64))
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::U8 => self.build_output_stream::<u8, _, _>(
                &format.config(),
                move |data, _| {
                    data.iter_mut().for_each(|d| {
                        *d = mixer_rx
                            .next()
                            .map(Sample::from_sample)
                            .unwrap_or(u8::max_value() / 2)
                    })
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::U16 => self.build_output_stream::<u16, _, _>(
                &format.config(),
                move |data, _| {
                    data.iter_mut().for_each(|d| {
                        *d = mixer_rx
                            .next()
                            .map(Sample::from_sample)
                            .unwrap_or(u16::max_value() / 2)
                    })
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::U32 => self.build_output_stream::<u32, _, _>(
                &format.config(),
                move |data, _| {
                    data.iter_mut().for_each(|d| {
                        *d = mixer_rx
                            .next()
                            .map(Sample::from_sample)
                            .unwrap_or(u32::max_value() / 2)
                    })
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::U64 => self.build_output_stream::<u64, _, _>(
                &format.config(),
                move |data, _| {
                    data.iter_mut().for_each(|d| {
                        *d = mixer_rx
                            .next()
                            .map(Sample::from_sample)
                            .unwrap_or(u64::max_value() / 2)
                    })
                },
                error_callback,
                None,
            ),
            _ => return Err(cpal::BuildStreamError::StreamConfigNotSupported),
        }
        .map(|stream| (mixer_tx, stream))
    }

    fn try_new_output_stream_config(
        &self,
        config: SupportedStreamConfig,
    ) -> Result<(Arc<DynamicMixerController<f32>>, cpal::Stream), StreamError> {
        self.new_output_stream_with_format(config).or_else(|err| {
            // look through all supported formats to see if another works
            supported_output_formats(self)?
                .find_map(|format| self.new_output_stream_with_format(format).ok())
                // return original error if nothing works
                .ok_or(StreamError::BuildStreamError(err))
        })
    }
}

/// All the supported output formats with sample rates
fn supported_output_formats(
    device: &cpal::Device,
) -> Result<impl Iterator<Item = cpal::SupportedStreamConfig>, StreamError> {
    const HZ_44100: cpal::SampleRate = cpal::SampleRate(44_100);

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
