use cpal::{traits::DeviceTrait, Sample};
use dynamic_mixer::{self, DynamicMixer, DynamicMixerController};
use std::sync::Arc;

/// Extensions to `cpal::Device`
pub(crate) trait RodioDevice {
    fn new_output_stream_with_format(
        &self,
        format: cpal::Format,
    ) -> Result<(Arc<DynamicMixerController<f32>>, cpal::Stream), cpal::BuildStreamError>;

    fn new_output_stream(&self) -> (Arc<DynamicMixerController<f32>>, cpal::Stream);
}

impl RodioDevice for cpal::Device {
    fn new_output_stream_with_format(
        &self,
        format: cpal::Format,
    ) -> Result<(Arc<DynamicMixerController<f32>>, cpal::Stream), cpal::BuildStreamError> {
        let (mixer_tx, mut mixer_rx) =
            dynamic_mixer::mixer::<f32>(format.channels, format.sample_rate.0);

        self.build_output_stream(
            &format,
            move |data| audio_callback(&mut mixer_rx, data),
            move |err| eprintln!("an error occurred on output stream: {}", err),
        )
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

fn audio_callback(mixer: &mut DynamicMixer<f32>, buffer: cpal::StreamData) {
    use cpal::{StreamData, UnknownTypeOutputBuffer};

    match buffer {
        StreamData::Output {
            buffer: UnknownTypeOutputBuffer::U16(mut buffer),
        } => {
            for d in buffer.iter_mut() {
                *d = mixer
                    .next()
                    .map(|s| s.to_u16())
                    .unwrap_or(u16::max_value() / 2);
            }
        }
        StreamData::Output {
            buffer: UnknownTypeOutputBuffer::I16(mut buffer),
        } => {
            for d in buffer.iter_mut() {
                *d = mixer.next().map(|s| s.to_i16()).unwrap_or(0i16);
            }
        }
        StreamData::Output {
            buffer: UnknownTypeOutputBuffer::F32(mut buffer),
        } => {
            for d in buffer.iter_mut() {
                *d = mixer.next().unwrap_or(0f32);
            }
        }
        StreamData::Input { .. } => {
            panic!("Can't play an input stream!");
        }
    };
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
