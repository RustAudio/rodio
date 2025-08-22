use std::{marker::PhantomData, time::Duration};

use cpal::{
    traits::{DeviceTrait, HostTrait},
    SupportedStreamConfigRange,
};
use rtrb::RingBuffer;

use crate::{
    conversions::SampleTypeConverter, microphone::config::InputConfig, ChannelCount, SampleRate,
};

use super::Microphone;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("")]
    NoDevice,
    #[error("")]
    DefaultInputConfig(#[source] cpal::DefaultStreamConfigError),
    #[error("")]
    InputConfigs(#[source] cpal::SupportedStreamConfigsError),
    #[error("")]
    UnsupportedByDevice,
}

pub trait ToAssign {}

pub struct DeviceIsSet;
pub struct DeviceNotSet;
impl ToAssign for DeviceIsSet {}
impl ToAssign for DeviceNotSet {}

pub struct ConfigIsSet;
pub struct ConfigNotSet;
impl ToAssign for ConfigIsSet {}
impl ToAssign for ConfigNotSet {}

#[must_use]
pub struct MicrophoneBuilder<Device, Config, E = fn(cpal::StreamError)>
where
    Device: ToAssign,
    Config: ToAssign,
    E: FnMut(cpal::StreamError) + Send + Clone + 'static,
{
    device: Option<(cpal::Device, Vec<SupportedStreamConfigRange>)>,
    config: Option<super::config::InputConfig>,
    error_callback: E,

    device_set: PhantomData<Device>,
    config_set: PhantomData<Config>,
}

impl Default for MicrophoneBuilder<DeviceNotSet, ConfigNotSet> {
    fn default() -> Self {
        Self {
            device: None,
            config: None,
            error_callback: default_error_callback,

            device_set: PhantomData::default(),
            config_set: PhantomData::default(),
        }
    }
}

fn default_error_callback(err: cpal::StreamError) {
    #[cfg(feature = "tracing")]
    tracing::error!("audio stream error: {err}");
    #[cfg(not(feature = "tracing"))]
    eprintln!("audio stream error: {err}");
}

impl MicrophoneBuilder<DeviceNotSet, ConfigNotSet, fn(cpal::StreamError)> {
    pub fn new() -> MicrophoneBuilder<DeviceNotSet, ConfigNotSet, fn(cpal::StreamError)> {
        Self {
            device: None,
            config: None,
            error_callback: default_error_callback,

            device_set: PhantomData::default(),
            config_set: PhantomData::default(),
        }
    }
}

impl<Device, Config, E> MicrophoneBuilder<Device, Config, E>
where
    Device: ToAssign,
    Config: ToAssign,
    E: FnMut(cpal::StreamError) + Send + Clone + 'static,
{
    /// Sets output device and its default parameters.
    pub fn device(
        self,
        device: cpal::Device,
    ) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigNotSet, E>, Error> {
        let supported_configs = device
            .supported_input_configs()
            .map_err(Error::InputConfigs)?
            .collect();
        Ok(MicrophoneBuilder {
            device: Some((device, supported_configs)),
            config: self.config,
            error_callback: self.error_callback,
            device_set: PhantomData::default(),
            config_set: PhantomData::default(),
        })
    }

    pub fn default_device(self) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigNotSet, E>, Error> {
        let default_device = cpal::default_host()
            .default_output_device()
            .ok_or(Error::NoDevice)?;
        let supported_configs = default_device
            .supported_input_configs()
            .map_err(Error::InputConfigs)?
            .collect();
        Ok(MicrophoneBuilder {
            device: Some((default_device, supported_configs)),
            config: self.config,
            error_callback: self.error_callback,
            device_set: PhantomData::default(),
            config_set: PhantomData::default(),
        })
    }
}

impl<Config, E> MicrophoneBuilder<DeviceIsSet, Config, E>
where
    Config: ToAssign,
    E: FnMut(cpal::StreamError) + Send + Clone + 'static,
{
    pub fn default_config(self) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E>, Error> {
        let default_config = self
            .device
            .as_ref()
            .expect("DeviceIsSet")
            .0
            .default_input_config()
            .map_err(Error::DefaultInputConfig)?;
        Ok(MicrophoneBuilder {
            device: self.device,
            config: Some(default_config.into()),
            error_callback: self.error_callback,
            device_set: PhantomData::default(),
            config_set: PhantomData::default(),
        })
    }

    pub fn config(
        self,
        config: InputConfig,
    ) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E>, Error> {
        self.check_config(&config)?;

        Ok(MicrophoneBuilder {
            device: self.device,
            config: Some(config),
            error_callback: self.error_callback,
            device_set: PhantomData::default(),
            config_set: PhantomData::default(),
        })
    }

    fn check_config(&self, config: &InputConfig) -> Result<(), Error> {
        if !self
            .device
            .as_ref()
            .expect("DeviceIsSet")
            .1
            .iter()
            .any(|range| config.supported_given(range))
        {
            return Err(Error::UnsupportedByDevice);
        } else {
            Ok(())
        }
    }

    pub fn samplerate(
        self,
        sample_rate: SampleRate,
    ) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E>, Error> {
        let mut new_config = self.config.expect("ConfigIsSet");
        new_config.sample_rate = sample_rate;
        self.check_config(&new_config)?;

        Ok(MicrophoneBuilder {
            device: self.device,
            config: Some(new_config),
            error_callback: self.error_callback,
            device_set: PhantomData::default(),
            config_set: PhantomData::default(),
        })
    }

    pub fn channels(
        self,
        channel_count: ChannelCount,
    ) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E>, Error> {
        let mut new_config = self.config.expect("ConfigIsSet");
        new_config.channel_count = channel_count;
        self.check_config(&new_config)?;

        Ok(MicrophoneBuilder {
            device: self.device,
            config: Some(new_config),
            error_callback: self.error_callback,
            device_set: PhantomData::default(),
            config_set: PhantomData::default(),
        })
    }

    pub fn buffer_size(
        self,
        buffer_size: u32,
    ) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E>, Error> {
        let mut new_config = self.config.expect("ConfigIsSet");
        new_config.buffer_size = cpal::BufferSize::Fixed(buffer_size);
        self.check_config(&new_config)?;

        Ok(MicrophoneBuilder {
            device: self.device,
            config: Some(new_config),
            error_callback: self.error_callback,
            device_set: PhantomData::default(),
            config_set: PhantomData::default(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("Could not open microphone")]
    StreamError(#[source] cpal::BuildStreamError),
    #[error("This is a bug, please report it")]
    UnsupportedSampleFormat,
}

impl<E> MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E>
where
    E: FnMut(cpal::StreamError) + Send + Clone + 'static,
{
    pub fn open_stream(&self) -> Result<Microphone, OpenError> {
        let device = self.device.as_ref().expect("DeviceIsSet").0.clone();
        let config = self.config.as_ref().expect("ConfigIsSet").clone();

        let timeout = Some(Duration::from_millis(100));
        let (mut tx, rx) = RingBuffer::new(1024);

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
                            self.error_callback.clone(),
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
        );

        Ok(Microphone {
            stream_handle: stream.map_err(OpenError::StreamError)?,
            buffer: rx,
            channels: config.channel_count,
            sample_rate: config.sample_rate,
        })
    }
}

// fn capture_input(
//     apm: Arc<Mutex<apm::AudioProcessingModule>>,
//     frame_tx: UnboundedSender<AudioFrame<'static>>,
//     sample_rate: u32,
//     num_channels: u32,
// ) -> Result {
//     loop {
//         let mut device_change_listener = DeviceChangeListener::new(true)?;
//         let (device, config) = crate::default_device(true)?;
//         let (end_on_drop_tx, end_on_drop_rx) = std::sync::mpsc::channel::<()>();
//         let apm = apm.clone();
//         let frame_tx = frame_tx.clone();
//         let mut resampler = audio_resampler::AudioResampler::default();

//         thread::spawn(move || {
//             maybe!({
//                 if let Some(name) = device.name().ok() {
//                     log::info!("Using microphone: {}", name)
//                 } else {
//                     log::info!("Using microphone: <unknown>");
//                 }

//                 let ten_ms_buffer_size =
//                     (config.channels() as u32 * config.sample_rate().0 / 100) as usize;
//                 let mut buf: Vec<i16> = Vec::with_capacity(ten_ms_buffer_size);

//                 let stream = device
//                     .build_input_stream_raw(
//                         &config.config(),
//                         config.sample_format(),
//                         move |data, _: &_| {
//                             let data =
//                                 crate::get_sample_data(config.sample_format(), data).log_err();
//                             let Some(data) = data else {
//                                 return;
//                             };
//                             let mut data = data.as_slice();

//                             while data.len() > 0 {
//                                 let remainder = (buf.capacity() - buf.len()).min(data.len());
//                                 buf.extend_from_slice(&data[..remainder]);
//                                 data = &data[remainder..];

//                                 if buf.capacity() == buf.len() {
//                                     let mut sampled = resampler
//                                         .remix_and_resample(
//                                             buf.as_slice(),
//                                             config.sample_rate().0 / 100,
//                                             config.channels() as u32,
//                                             config.sample_rate().0,
//                                             num_channels,
//                                             sample_rate,
//                                         )
//                                         .to_owned();
//                                     apm.lock()
//                                         .process_stream(
//                                             &mut sampled,
//                                             sample_rate as i32,
//                                             num_channels as i32,
//                                         )
//                                         .log_err();
//                                     buf.clear();
//                                     frame_tx
//                                         .unbounded_send(AudioFrame {
//                                             data: Cow::Owned(sampled),
//                                             sample_rate,
//                                             num_channels,
//                                             samples_per_channel: sample_rate / 100,
//                                         })
//                                         .ok();
//                                 }
//                             }
//                         },
//                         |err| log::error!("error capturing audio track: {:?}", err),
//                         Some(Duration::from_millis(100)),
//                     )
//                     .context("failed to build input stream")?;

//                 stream.play()?;
//                 // Keep the thread alive and holding onto the `stream`
//                 end_on_drop_rx.recv().ok();
//                 anyhow::Ok(Some(()))
//             })
//             .log_err();
//         });

//         device_change_listener.next().await;
//         drop(end_on_drop_tx)
//     }
// }
