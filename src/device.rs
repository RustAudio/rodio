use crate::decoder;
use crate::device_mixer::DeviceMixer;
use crate::dynamic_mixer::{self, DynamicMixerController};
use crate::sink::Sink;
use crate::source::Source;
use cpal::{
    traits::{DeviceTrait, HostTrait},
    Sample,
};
use std::cell::RefCell;
use std::io::{Read, Seek};
use std::sync::Arc;

pub struct RodioDevice {
    mixer: RefCell<DeviceMixer>,
    inner: cpal::Device,
}

impl From<cpal::Device> for RodioDevice {
    fn from(device: cpal::Device) -> Self {
        Self {
            inner: device,
            mixer: <_>::default(),
        }
    }
}

impl RodioDevice {
    pub fn default_output() -> Option<Self> {
        Some(cpal::default_host().default_output_device()?.into())
    }

    /// Plays a source with a device until it ends.
    pub fn play_raw<S>(&self, source: S)
    where
        S: Source<Item = f32> + Send + 'static,
    {
        self.mixer.borrow_mut().play(&self.inner, source)
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

/// Extensions to `cpal::Device`
pub(crate) trait CpalDeviceExt {
    fn new_output_stream_with_format(
        &self,
        format: cpal::Format,
    ) -> Result<(Arc<DynamicMixerController<f32>>, cpal::Stream), cpal::BuildStreamError>;

    fn new_output_stream(&self) -> (Arc<DynamicMixerController<f32>>, cpal::Stream);
}

impl CpalDeviceExt for cpal::Device {
    fn new_output_stream_with_format(
        &self,
        format: cpal::Format,
    ) -> Result<(Arc<DynamicMixerController<f32>>, cpal::Stream), cpal::BuildStreamError> {
        let (mixer_tx, mut mixer_rx) =
            dynamic_mixer::mixer::<f32>(format.channels, format.sample_rate.0);

        let error_callback = |err| eprintln!("an error occurred on output stream: {}", err);

        match format.data_type {
            cpal::SampleFormat::F32 => self.build_output_stream::<f32, _, _>(
                &format.shape(),
                move |data| {
                    data.iter_mut()
                        .for_each(|d| *d = mixer_rx.next().unwrap_or(0f32))
                },
                error_callback,
            ),
            cpal::SampleFormat::I16 => self.build_output_stream::<i16, _, _>(
                &format.shape(),
                move |data| {
                    data.iter_mut()
                        .for_each(|d| *d = mixer_rx.next().map(|s| s.to_i16()).unwrap_or(0i16))
                },
                error_callback,
            ),
            cpal::SampleFormat::U16 => self.build_output_stream::<u16, _, _>(
                &format.shape(),
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
            .default_output_format()
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
fn supported_output_formats(device: &cpal::Device) -> impl Iterator<Item = cpal::Format> {
    const HZ_44100: cpal::SampleRate = cpal::SampleRate(44_100);

    let mut supported: Vec<_> = device
        .supported_output_formats()
        .expect("No supported output formats")
        .collect();
    supported.sort_by(|a, b| b.cmp_default_heuristics(a));

    supported.into_iter().flat_map(|sf| {
        let max_rate = sf.max_sample_rate;
        let min_rate = sf.min_sample_rate;
        let mut formats = vec![sf.clone().with_max_sample_rate()];
        if HZ_44100 < max_rate && HZ_44100 > min_rate {
            formats.push(sf.clone().with_sample_rate(HZ_44100))
        }
        formats.push(sf.with_sample_rate(min_rate));
        formats
    })
}

trait SupportedFormatExt {
    fn with_sample_rate(self, sample_rate: cpal::SampleRate) -> cpal::Format;
}
impl SupportedFormatExt for cpal::SupportedFormat {
    fn with_sample_rate(self, sample_rate: cpal::SampleRate) -> cpal::Format {
        let Self {
            channels,
            data_type,
            ..
        } = self;
        cpal::Format {
            channels,
            sample_rate,
            data_type,
        }
    }
}
