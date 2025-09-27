use std::{fmt::Debug, marker::PhantomData};

use cpal::{
    traits::{DeviceTrait, HostTrait},
    SupportedStreamConfigRange,
};

use crate::{
    common::assert_error_traits,
    microphone::{config::InputConfig, sendable},
    ChannelCount, SampleRate,
};

use super::Microphone;

/// Error configuring or opening microphone input
#[allow(missing_docs)]
#[derive(Debug, thiserror::Error, Clone)]
pub enum Error {
    /// No input device is available on the system.
    #[error("There is no input device")]
    NoDevice,
    /// Failed to get the default input configuration for the device.
    #[error("Could not get default input configuration for input device: '{device_name}'")]
    DefaultInputConfig {
        #[source]
        source: cpal::DefaultStreamConfigError,
        device_name: String,
    },
    /// Failed to get the supported input configurations for the device.
    #[error("Could not get supported input configurations for input device: '{device_name}'")]
    InputConfigs {
        #[source]
        source: cpal::SupportedStreamConfigsError,
        device_name: String,
    },
    /// The requested input configuration is not supported by the device.
    #[error("The input configuration is not supported by input device: '{device_name}'")]
    UnsupportedByDevice { device_name: String },
}
assert_error_traits! {Error}

/// Generic on the `MicrophoneBuilder` which is only present when a config has been set.
/// Methods needing a config are only available on MicrophoneBuilder with this
/// Generic set.
pub struct DeviceIsSet;
/// Generic on the `MicrophoneBuilder` which is only present when a device has been set.
/// Methods needing a device set are only available on MicrophoneBuilder with this
/// Generic set.
pub struct ConfigIsSet;

/// Generic on the `MicrophoneBuilder` which indicates no config has been set.
/// Some methods are only available when this types counterpart: `ConfigIsSet` is present.
pub struct ConfigNotSet;
/// Generic on the `MicrophoneBuilder` which indicates no device has been set.
/// Some methods are only available when this types counterpart: `DeviceIsSet` is present.
pub struct DeviceNotSet;

/// Builder for configuring and opening microphone input streams.
#[must_use]
pub struct MicrophoneBuilder<Device, Config, E = fn(cpal::StreamError)>
where
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

            device_set: PhantomData,
            config_set: PhantomData,
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
    /// Creates a new microphone builder.
    ///
    /// # Example
    /// ```no_run
    /// let builder = rodio::microphone::MicrophoneBuilder::new();
    /// ```
    pub fn new() -> MicrophoneBuilder<DeviceNotSet, ConfigNotSet, fn(cpal::StreamError)> {
        Self::default()
    }
}

impl<Device, Config, E> MicrophoneBuilder<Device, Config, E>
where
    E: FnMut(cpal::StreamError) + Send + Clone + 'static,
{
    /// Sets the input device to use.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::microphone::{MicrophoneBuilder, available_inputs};
    /// let input = available_inputs()?.remove(2);
    /// let builder = MicrophoneBuilder::new().device(input)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn device(
        &self,
        device: impl Into<cpal::Device>,
    ) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigNotSet, E>, Error> {
        let device = device.into();
        let supported_configs = device
            .supported_input_configs()
            .map_err(|source| Error::InputConfigs {
                source,
                device_name: device.name().unwrap_or_else(|_| "unknown".to_string()),
            })?
            .collect();
        Ok(MicrophoneBuilder {
            device: Some((device, supported_configs)),
            config: self.config,
            error_callback: self.error_callback.clone(),
            device_set: PhantomData,
            config_set: PhantomData,
        })
    }

    /// Uses the system's default input device.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::microphone::MicrophoneBuilder;
    /// let builder = MicrophoneBuilder::new().default_device()?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn default_device(&self) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigNotSet, E>, Error> {
        let default_device = cpal::default_host()
            .default_input_device()
            .ok_or(Error::NoDevice)?;
        let supported_configs = default_device
            .supported_input_configs()
            .map_err(|source| Error::InputConfigs {
                source,
                device_name: default_device
                    .name()
                    .unwrap_or_else(|_| "unknown".to_string()),
            })?
            .collect();
        Ok(MicrophoneBuilder {
            device: Some((default_device, supported_configs)),
            config: self.config,
            error_callback: self.error_callback.clone(),
            device_set: PhantomData,
            config_set: PhantomData,
        })
    }
}

impl<Config, E> MicrophoneBuilder<DeviceIsSet, Config, E>
where
    E: FnMut(cpal::StreamError) + Send + Clone + 'static,
{
    /// Uses the device's default input configuration.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::microphone::MicrophoneBuilder;
    /// let builder = MicrophoneBuilder::new()
    ///     .default_device()?
    ///     .default_config()?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn default_config(&self) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E>, Error> {
        let device = &self.device.as_ref().expect("DeviceIsSet").0;
        let default_config: InputConfig = device
            .default_input_config()
            .map_err(|source| Error::DefaultInputConfig {
                source,
                device_name: device.name().unwrap_or_else(|_| "unknown".to_string()),
            })?
            .into();

        // Lets try getting f32 output from the default config, as thats
        // what rodio uses internally
        let config = if self
            .check_config(&default_config.with_f32_samples())
            .is_ok()
        {
            default_config.with_f32_samples()
        } else {
            default_config
        };

        Ok(MicrophoneBuilder {
            device: self.device.clone(),
            config: Some(config),
            error_callback: self.error_callback.clone(),
            device_set: PhantomData,
            config_set: PhantomData,
        })
    }

    /// Sets a custom input configuration.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::microphone::{MicrophoneBuilder, InputConfig};
    /// # use std::num::NonZero;
    /// let config = InputConfig {
    ///     sample_rate: NonZero::new(44_100).expect("44100 is not zero"),
    ///     channel_count: NonZero::new(2).expect("2 is not zero"),
    ///     buffer_size: cpal::BufferSize::Fixed(42_000),
    ///     sample_format: cpal::SampleFormat::U16,
    /// };
    /// let builder = MicrophoneBuilder::new()
    ///     .default_device()?
    ///     .config(config)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn config(
        &self,
        config: InputConfig,
    ) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E>, Error> {
        self.check_config(&config)?;

        Ok(MicrophoneBuilder {
            device: self.device.clone(),
            config: Some(config),
            error_callback: self.error_callback.clone(),
            device_set: PhantomData,
            config_set: PhantomData,
        })
    }

    fn check_config(&self, config: &InputConfig) -> Result<(), Error> {
        let (device, supported_configs) = self.device.as_ref().expect("DeviceIsSet");
        if !supported_configs
            .iter()
            .any(|range| config.supported_given(range))
        {
            Err(Error::UnsupportedByDevice {
                device_name: device.name().unwrap_or_else(|_| "unknown".to_string()),
            })
        } else {
            Ok(())
        }
    }

    /// Sets the sample rate for input.
    ///
    /// # Error
    /// Returns an error if the requested sample rate combined with the
    /// other parameters can not be supported.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::microphone::MicrophoneBuilder;
    /// let builder = MicrophoneBuilder::new()
    ///     .default_device()?
    ///     .default_config()?
    ///     .try_sample_rate(44_100.try_into()?)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_sample_rate(
        &self,
        sample_rate: SampleRate,
    ) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E>, Error> {
        let mut new_config = self.config.expect("ConfigIsSet");
        new_config.sample_rate = sample_rate;
        self.check_config(&new_config)?;

        Ok(MicrophoneBuilder {
            device: self.device.clone(),
            config: Some(new_config),
            error_callback: self.error_callback.clone(),
            device_set: PhantomData,
            config_set: PhantomData,
        })
    }

    /// Try multiple sample rates, fall back to the default it non match. The
    /// sample rates are in order of preference. If the first can be supported
    /// the second will never be tried.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::microphone::MicrophoneBuilder;
    /// let builder = MicrophoneBuilder::new()
    ///     .default_device()?
    ///     .default_config()?
    ///     // 16k or its double with can trivially be resampled to 16k
    ///     .prefer_sample_rates([
    ///         16_000.try_into().expect("not zero"),
    ///         32_000.try_into().expect("not_zero"),
    ///     ]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn prefer_sample_rates(
        &self,
        sample_rates: impl IntoIterator<Item = SampleRate>,
    ) -> MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E> {
        self.set_preferred_if_supported(sample_rates, |config, sample_rate| {
            config.sample_rate = sample_rate
        })
    }

    fn set_preferred_if_supported<T>(
        &self,
        options: impl IntoIterator<Item = T>,
        setter: impl Fn(&mut InputConfig, T),
    ) -> MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E> {
        let mut config = self.config.expect("ConfigIsSet");
        let mut final_config = config;

        for option in options.into_iter() {
            setter(&mut config, option);
            if self.check_config(&config).is_ok() {
                final_config = config;
                break;
            }
        }

        MicrophoneBuilder {
            device: self.device.clone(),
            config: Some(final_config),
            error_callback: self.error_callback.clone(),
            device_set: PhantomData,
            config_set: PhantomData,
        }
    }

    /// Sets the number of input channels.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::microphone::MicrophoneBuilder;
    /// let builder = MicrophoneBuilder::new()
    ///     .default_device()?
    ///     .default_config()?
    ///     .try_channels(2.try_into()?)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_channels(
        &self,
        channel_count: ChannelCount,
    ) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E>, Error> {
        let mut new_config = self.config.expect("ConfigIsSet");
        new_config.channel_count = channel_count;
        self.check_config(&new_config)?;

        Ok(MicrophoneBuilder {
            device: self.device.clone(),
            config: Some(new_config),
            error_callback: self.error_callback.clone(),
            device_set: PhantomData,
            config_set: PhantomData,
        })
    }

    /// Try multiple channel counts, fall back to the default it non match. The
    /// channel counts are in order of preference. If the first can be supported
    /// the second will never be tried.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::microphone::MicrophoneBuilder;
    /// let builder = MicrophoneBuilder::new()
    ///     .default_device()?
    ///     .default_config()?
    ///     // We want mono, if thats not possible give
    ///     // us the lowest channel count
    ///     .prefer_channel_counts([
    ///         1.try_into().expect("not zero"),
    ///         2.try_into().expect("not_zero"),
    ///         3.try_into().expect("not_zero"),
    ///     ]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn prefer_channel_counts(
        &self,
        channel_counts: impl IntoIterator<Item = ChannelCount>,
    ) -> MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E> {
        self.set_preferred_if_supported(channel_counts, |config, count| {
            config.channel_count = count
        })
    }

    /// Sets the buffer size for the input.
    ///
    /// This has no impact on latency, though a too small buffer can lead to audio
    /// artifacts if your program can not get samples out of the buffer before they
    /// get overridden again.
    ///
    /// Normally the default input config will have this set up correctly.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::microphone::MicrophoneBuilder;
    /// let builder = MicrophoneBuilder::new()
    ///     .default_device()?
    ///     .default_config()?
    ///     .try_buffer_size(4096)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_buffer_size(
        &self,
        buffer_size: u32,
    ) -> Result<MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E>, Error> {
        let mut new_config = self.config.expect("ConfigIsSet");
        new_config.buffer_size = cpal::BufferSize::Fixed(buffer_size);
        self.check_config(&new_config)?;

        Ok(MicrophoneBuilder {
            device: self.device.clone(),
            config: Some(new_config),
            error_callback: self.error_callback.clone(),
            device_set: PhantomData,
            config_set: PhantomData,
        })
    }

    /// See the docs of [`try_buffer_size`](MicrophoneBuilder::try_buffer_size)
    /// for more.
    ///
    /// Try multiple buffer sizes, fall back to the default it non match. The
    /// buffer sizes are in order of preference. If the first can be supported
    /// the second will never be tried.
    ///
    /// # Note
    /// We will not try buffer sizes larger then 100_000 to prevent this
    /// from hanging too long on open ranges.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::microphone::MicrophoneBuilder;
    /// let builder = MicrophoneBuilder::new()
    ///     .default_device()?
    ///     .default_config()?
    ///     // We want mono, if thats not possible give
    ///     // us the lowest channel count
    ///     .prefer_buffer_sizes([
    ///         2048.try_into().expect("not zero"),
    ///         4096.try_into().expect("not_zero"),
    ///     ]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// Get the smallest buffer size larger then 512.
    /// ```no_run
    /// # use rodio::microphone::MicrophoneBuilder;
    /// let builder = MicrophoneBuilder::new()
    ///     .default_device()?
    ///     .default_config()?
    ///     // We want mono, if thats not possible give
    ///     // us the lowest channel count
    ///     .prefer_buffer_sizes(4096..);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn prefer_buffer_sizes(
        &self,
        buffer_sizes: impl IntoIterator<Item = u32>,
    ) -> MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E> {
        let buffer_sizes = buffer_sizes.into_iter().take_while(|size| *size < 100_000);

        self.set_preferred_if_supported(buffer_sizes, |config, size| {
            config.buffer_size = cpal::BufferSize::Fixed(size)
        })
    }
}

impl<Device, E> MicrophoneBuilder<Device, ConfigIsSet, E>
where
    E: FnMut(cpal::StreamError) + Send + Clone + 'static,
{
    /// Returns the current input configuration.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::microphone::MicrophoneBuilder;
    /// let builder = MicrophoneBuilder::new()
    ///     .default_device()?
    ///     .default_config()?;
    /// let config = builder.get_config();
    /// println!("Sample rate: {}", config.sample_rate.get());
    /// println!("Channel count: {}", config.channel_count.get());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn get_config(&self) -> &InputConfig {
        self.config.as_ref().expect("ConfigIsSet")
    }
}

impl<E> MicrophoneBuilder<DeviceIsSet, ConfigIsSet, E>
where
    E: FnMut(cpal::StreamError) + Send + Clone + 'static,
{
    /// Opens the microphone input stream.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::microphone::MicrophoneBuilder;
    /// # use rodio::Source;
    /// # use std::time::Duration;
    /// let mic = MicrophoneBuilder::new()
    ///     .default_device()?
    ///     .default_config()?
    ///     .open_stream()?;
    /// let recording = mic.take_duration(Duration::from_secs(3)).record();
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn open_stream(&self) -> Result<Microphone, super::OpenError> {
        Microphone::open(
            self.device.as_ref().expect("DeviceIsSet").0.clone(),
            *self.config.as_ref().expect("ConfigIsSet"),
            self.error_callback.clone(),
        )
    }
    /// Opens the microphone input stream.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::microphone::MicrophoneBuilder;
    /// # use rodio::Source;
    /// # use std::time::Duration;
    /// let mic = MicrophoneBuilder::new()
    ///     .default_device()?
    ///     .default_config()?
    ///     .open_stream()?;
    /// let recording = mic.take_duration(Duration::from_secs(3)).record();
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn open_sendable_stream(&self) -> Result<sendable::Microphone, super::OpenError> {
        sendable::Microphone::open(
            self.device.as_ref().expect("DeviceIsSet").0.clone(),
            *self.config.as_ref().expect("ConfigIsSet"),
            self.error_callback.clone(),
        )
    }
}
