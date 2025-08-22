use std::{fmt::Debug, marker::PhantomData};

use cpal::{
    traits::{DeviceTrait, HostTrait},
    SupportedStreamConfigRange,
};

use crate::{microphone::config::InputConfig, ChannelCount, SampleRate};

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

impl<Device, Config, E> Debug for MicrophoneBuilder<Device, Config, E>
where
    Device: ToAssign,
    Config: ToAssign,
    E: FnMut(cpal::StreamError) + Send + Clone + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicrophoneBuilder")
            .field(
                "device",
                &self
                    .device
                    .as_ref()
                    .map(|d| d.0.name().unwrap_or("unknown".to_string())),
            )
            .field("config", &self.config)
            .finish()
    }
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
        Self::default()
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
        device: impl Into<cpal::Device>,
    ) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigNotSet, E>, Error> {
        let device = device.into();
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

impl<E> MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E>
where
    E: FnMut(cpal::StreamError) + Send + Clone + 'static,
{
    pub fn open_stream(&self) -> Result<Microphone, super::OpenError> {
        Microphone::open(
            self.device.as_ref().expect("DeviceIsSet").0.clone(),
            self.config.as_ref().expect("ConfigIsSet").clone(),
            self.error_callback.clone(),
        )
    }
}
