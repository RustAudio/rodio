//! A speakers sink
//!
//! An audio *stream* originates at a [`Source`](crate::Source) and flows to a player. This is a
//! Sink that plays audio over the systems speakers or headphones through an
//! audio output device;
//!
//! # Basic Usage
//!
//! ```no_run
//! # use rodio::speakers::SpeakersBuilder;
//! # use rodio::{Source, source::SineWave};
//! # use std::time::Duration;
//! let speakers = SpeakersBuilder::new()
//!     .default_device()?
//!     .default_config()?
//!     .open_mixer()?;
//! let mixer = speakers.mixer();
//!
//! // Play a beep for 4 seconds
//! mixer.add(SineWave::new(440.).take_duration(Duration::from_secs(4)));
//! std::thread::sleep(Duration::from_secs(4));
//!
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Use preferred parameters if supported
//! Attempt to set a specific channel count, sample rate and buffer size but
//! fall back to the default if the device does not support these
//!
//! ```no_run
//! use rodio::speakers::SpeakersBuilder;
//! use rodio::Source;
//! use std::time::Duration;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut builder = SpeakersBuilder::new()
//!     .default_device()?
//!     .default_config()?
//!     .prefer_channel_counts([
//!         1.try_into().expect("not zero"),
//!         2.try_into().expect("not zero"),
//!     ])
//!     .prefer_sample_rates([
//!         16_000.try_into().expect("not zero"),
//!         32_000.try_into().expect("not zero"),
//!     ])
//!     .prefer_buffer_sizes(512..);
//!
//! let mixer = builder.open_mixer()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Configuration with Error Handling
//! Attempt to set a specific channel count but fall back to the default if
//! the device doesn't support it:
//!
//! ```no_run
//! use rodio::speakers::SpeakersBuilder;
//! use rodio::Source;
//! use std::time::Duration;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut builder = SpeakersBuilder::new()
//!     .default_device()?
//!     .default_config()?;
//!
//! // Try to set stereo recording (2 channels), but continue with default if unsupported
//! if let Ok(configured_builder) = builder.try_channels(2.try_into()?) {
//!     builder = configured_builder;
//! } else {
//!     println!("Stereo recording not supported, using default channel configuration");
//!     // builder remains unchanged with default configuration
//! }
//!
//! let speakers = builder.open_mixer()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Device Selection
//!
//! ```no_run
//! use rodio::speakers::{SpeakersBuilder, available_outputs};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // List all available output devices
//! let outputs = available_outputs()?;
//! for (i, output) in outputs.iter().enumerate() {
//!     println!("output {}: {}", i, output);
//! }
//!
//! // Use a specific device (e.g., the second one)
//! let speakers = SpeakersBuilder::new()
//!     .device(outputs[1].clone())?
//!     .default_config()?
//!     .open_mixer()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Storing & Loading Output
//!
//! ```no_run
//! use cpal::DeviceId;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // load
//! let device_id = DeviceId::from_str("Some stored input")?;
//! let output = AvailableOutput::try_from(device_id)?;
//!
//! // store
//! let device_id = output.device_id()?;
//! let device_id = device_id.to_string();
//! # }
//! ```

use core::fmt;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::DeviceId;

use crate::common::assert_error_traits;

mod builder;
mod config;

pub use builder::SpeakersBuilder;
pub use config::{BufferSize, OutputConfig};

/// Error that can occur when we can not list the output devices
#[derive(Debug, thiserror::Error, Clone)]
#[error("Could not list output devices")]
pub struct ListError(#[source] cpal::DevicesError);
assert_error_traits! {ListError}

#[derive(Debug, thiserror::Error)]
pub enum NotAvailable {
    #[error(
        "The OS audio API the saved output used ({expected}) while supported on this system is not available."
    )]
    Host { expected: String },
    #[error("There is not device with id: {device_id} available on this system.")]
    NoDevice { device_id: DeviceId },
}

impl TryFrom<DeviceId> for AvailableOutput {
    type Error = NotAvailable;

    fn try_from(device_id: DeviceId) -> Result<Self, NotAvailable> {
        let host_id = device_id.0;
        let host = cpal::platform::host_from_id(host_id).map_err(|_| NotAvailable::Host {
            expected: host_id.to_string(),
        })?;
        let device = host
            .device_by_id(&device_id)
            .ok_or_else(|| NotAvailable::NoDevice {
                device_id: device_id.clone(),
            })?;
        let default_id = host.default_output_device().map(|d| d.id());

        Ok(Self {
            inner: device,
            default: default_id.is_some_and(|id| id.is_ok_and(|id| id == device_id)),
        })
    }
}

/// An output device
#[derive(Clone)]
pub struct AvailableOutput {
    inner: cpal::Device,
    default: bool,
}

impl AvailableOutput {
    /// Whether this output is the default sound output for the OS
    pub fn is_default(&self) -> bool {
        self.default
    }

    pub(crate) fn into_inner(self) -> cpal::Device {
        self.inner
    }

    pub fn device_id(&self) -> Result<DeviceId, cpal::DeviceIdError> {
        self.inner.id()
    }
}

impl fmt::Debug for AvailableOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field(
                "inner",
                &self
                    .inner
                    .description()
                    .map_or("unknown".to_string(), |d| d.name().to_string()),
            )
            .finish()
    }
}

impl fmt::Display for AvailableOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            self.inner
                .description()
                .map_or("unknown".to_string(), |d| d.name().to_string())
        )
    }
}

/// Returns a list of available output devices on the system.
pub fn available_outputs() -> Result<Vec<AvailableOutput>, ListError> {
    let host = cpal::default_host();
    let default = host.default_output_device().map(|d| d.id());
    let devices = host
        .output_devices()
        .map_err(ListError)?
        .map(|dev| AvailableOutput {
            default: Some(dev.id()) == default,
            inner: dev,
        });
    Ok(devices.collect())
}
