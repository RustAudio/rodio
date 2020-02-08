use crate::decoder;
use crate::dynamic_mixer::{self, DynamicMixerController};
use crate::sink::Sink;
use crate::source::Source;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Sample,
};
use std::convert::TryFrom;
use std::io::{Read, Seek};
use std::sync::Arc;
use std::{error, fmt};

pub struct OutputStream {
    mixer: Arc<DynamicMixerController<f32>>,
    _stream: cpal::Stream,
}

impl TryFrom<&'_ cpal::Device> for OutputStream {
    type Error = StreamError;

    fn try_from(device: &cpal::Device) -> Result<Self, Self::Error> {
        let (mixer, _stream) = device.new_output_stream();
        _stream.play()?;
        Ok(Self { mixer, _stream })
    }
}

impl OutputStream {
    pub fn try_default() -> Result<Self, StreamError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(StreamError::NoDevice)?;
        Self::try_from(&device)
    }

    /// Plays a source with a device until it ends.
    pub fn play_raw<S>(&self, source: S)
    where
        S: Source<Item = f32> + Send + 'static,
    {
        self.mixer.add(source);
    }

    /// Plays a sound once. Returns a `Sink` that can be used to control the sound.
    #[inline]
    pub fn play_once<R>(&self, input: R) -> Result<Sink, decoder::DecoderError>
    where
        R: Read + Seek + Send + 'static,
    {
        let input = decoder::Decoder::new(input)?;
        let sink = Sink::new(&self);
        sink.append(input);
        Ok(sink)
    }
}

#[derive(Debug)]
pub enum StreamError {
    PlayStreamError(cpal::PlayStreamError),
    NoDevice,
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
            Self::NoDevice => write!(f, "NoDevice"),
        }
    }
}

impl error::Error for StreamError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::PlayStreamError(e) => Some(e),
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

    fn new_output_stream(&self) -> (Arc<DynamicMixerController<f32>>, cpal::Stream);
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
                move |data| {
                    data.iter_mut()
                        .for_each(|d| *d = mixer_rx.next().unwrap_or(0f32))
                },
                error_callback,
            ),
            cpal::SampleFormat::I16 => self.build_output_stream::<i16, _, _>(
                &format.config(),
                move |data| {
                    data.iter_mut()
                        .for_each(|d| *d = mixer_rx.next().map(|s| s.to_i16()).unwrap_or(0i16))
                },
                error_callback,
            ),
            cpal::SampleFormat::U16 => self.build_output_stream::<u16, _, _>(
                &format.config(),
                move |data| {
                    data.iter_mut().for_each(|d| {
                        *d = mixer_rx
                            .next()
                            .map(|s| s.to_u16())
                            .unwrap_or(u16::max_value() / 2)
                    })
                },
                error_callback,
            ),
        }
        .map(|stream| (mixer_tx, stream))
    }

    fn new_output_stream(&self) -> (Arc<DynamicMixerController<f32>>, cpal::Stream) {
        // Determine the format to use for the new stream.
        let default_format = self
            .default_output_config()
            .expect("The device doesn't support any format!?");

        self.new_output_stream_with_format(default_format)
            .unwrap_or_else(|err| {
                // look through all supported formats to see if another works
                supported_output_formats(self)
                    .filter_map(|format| self.new_output_stream_with_format(format).ok())
                    .next()
                    .ok_or(err)
                    .expect("build_output_stream failed with all supported formats")
            })
    }
}

/// All the supported output formats with sample rates
fn supported_output_formats(
    device: &cpal::Device,
) -> impl Iterator<Item = cpal::SupportedStreamConfig> {
    const HZ_44100: cpal::SampleRate = cpal::SampleRate(44_100);

    let mut supported: Vec<_> = device
        .supported_output_configs()
        .expect("No supported output formats")
        .collect();
    supported.sort_by(|a, b| b.cmp_default_heuristics(a));

    supported.into_iter().flat_map(|sf| {
        let max_rate = sf.max_sample_rate();
        let min_rate = sf.min_sample_rate();
        let mut formats = vec![sf.clone().with_max_sample_rate()];
        if HZ_44100 < max_rate && HZ_44100 > min_rate {
            formats.push(sf.clone().with_sample_rate(HZ_44100))
        }
        formats.push(sf.with_sample_rate(min_rate));
        formats
    })
}
