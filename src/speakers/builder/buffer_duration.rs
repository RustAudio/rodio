use std::marker::PhantomData;
use std::ops::{Range, RangeFrom, RangeTo};
use std::time::Duration;

use cpal::traits::DeviceTrait;

use crate::speakers::builder::Error;
use crate::speakers::BufferSize;

use super::SpeakersBuilder;
use super::{ConfigIsSet, DeviceIsSet};

impl<E> SpeakersBuilder<DeviceIsSet, ConfigIsSet, E>
where
    E: FnMut(cpal::StreamError) + Send + Clone + 'static,
{
    /// Sets the buffer duration for the output. The buffer size is calculated
    /// from this and the sample rate and channel count when we build the
    /// output. Prefer this to [`SpeakersBuilder::try_buffer_size`].
    ///
    /// Long buffers will cause noticeable latency. A buffer that is too short
    /// however leads to audio artifacts when your machine can not generate
    /// a buffer of samples on time.
    ///
    /// Normally the default output config will have this set up correctly. You
    /// may want to tweak this to get lower latency or compensate for a
    /// inconsistent audio pipeline.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::speakers::SpeakersBuilder;
    /// # use std::time::Duration;
    /// let builder = SpeakersBuilder::new()
    ///     .default_device()?
    ///     .default_config()?
    ///     .try_buffer_duration(Duration::from_millis(20))?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_buffer_duration(
        &self,
        duration: Duration,
    ) -> Result<SpeakersBuilder<DeviceIsSet, ConfigIsSet, E>, Error> {
        let mut new_config = self.config.expect("ConfigIsSet");
        new_config.buffer_size = BufferSize::Duration(duration);
        self.check_config(&new_config)?;

        Ok(SpeakersBuilder {
            device: self.device.clone(),
            config: Some(new_config),
            error_callback: self.error_callback.clone(),
            device_set: PhantomData,
            config_set: PhantomData,
        })
    }

    /// See the docs of [`try_buffer_duration`](SpeakersBuilder::try_buffer_duration)
    /// for more.
    ///
    /// Try multiple buffer durations, fall back to the default if non match. The
    /// buffer durations are in order of preference. If the first can be supported
    /// the second will never be tried.
    ///
    /// # Note
    /// We will not try buffer durations longer then ten seconds to prevent this
    /// from hanging too long on open ranges.
    ///
    /// # Example
    /// ```no_run
    /// # use rodio::speakers::SpeakersBuilder;
    /// # use std::time::Duration;
    /// let builder = SpeakersBuilder::new()
    ///     .default_device()?
    ///     .default_config()?
    ///     .prefer_buffer_durations([
    ///         Duration::from_millis(10),
    ///         Duration::from_millis(50),
    ///     ]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// Get the smallest buffer that holds more then 10 ms of audio.
    /// ```no_run
    /// # use rodio::speakers::SpeakersBuilder;
    /// # use std::time::Duration;
    /// let builder = SpeakersBuilder::new()
    ///     .default_device()?
    ///     .default_config()?
    ///     .prefer_buffer_durations(Duration::from_millis(10)..);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn prefer_buffer_durations(
        &self,
        durations: impl IntoBufferSizeRange,
    ) -> Result<SpeakersBuilder<DeviceIsSet, ConfigIsSet, E>, Error> {
        let mut config = self.config.expect("ConfigIsSet");

        let (mut found_min, mut found_max) = (None, None);
        let (device, supported_configs) = self.device.as_ref().expect("DeviceIsSet");
        for supported in supported_configs {
            if config.channel_count.get() != supported.channels()
                || config.sample_format != supported.sample_format()
                || !(supported.min_sample_rate()..=supported.max_sample_rate())
                    .contains(&config.sample_rate.get())
            {
                continue;
            }

            if let cpal::SupportedBufferSize::Range { min, max } = supported.buffer_size() {
                found_min = found_min.min(Some(*min));
                found_max = found_max.max(Some(*max));
            };
        }

        // Sometimes an OS reports a crazy maximum that does not actually works
        // (we've spotted u32::MAX in the wild) but it will happily try and
        // break. Thus limit the buffer size to something sensible.
        let (min, max) = (
            found_min.unwrap_or(1),
            (found_max.unwrap_or(u32::MAX)).min(16384),
        );
        let min = Duration::from_secs_f64(min as f64 / config.sample_rate.get() as f64);
        let max = Duration::from_secs_f64(max as f64 / config.sample_rate.get() as f64);
        let supported = min..=max;

        use BufferSizeRange as B;
        let buffer_size = match &durations.into_buffer_size_range() {
            B::RangeFrom(RangeFrom { start }) if supported.contains(start) => Some(start),
            B::RangeFrom(RangeFrom { .. }) => Some(supported.start()),
            B::RangeTo(RangeTo { end }) if supported.start() > end => None,
            B::RangeTo(RangeTo { .. }) => Some(supported.start()),
            B::Range(Range { start, .. }) if supported.contains(start) => Some(start),
            B::Range(Range { end, .. }) if supported.contains(end) => Some(supported.start()),
            B::Range(Range { .. }) => None,
            B::Iter(durations) => durations.iter().find(|d| supported.contains(d)),
        }
        .copied()
        .ok_or(Error::UnsupportedByDevice {
            device_name: device
                .description()
                .map_or("unknown".to_string(), |d| d.name().to_string()),
        })?;

        config.buffer_size = BufferSize::Duration(buffer_size);
        Ok(SpeakersBuilder {
            device: self.device.clone(),
            config: Some(config),
            error_callback: self.error_callback.clone(),
            device_set: PhantomData,
            config_set: PhantomData,
        })
    }
}

pub enum BufferSizeRange {
    RangeFrom(RangeFrom<Duration>),
    RangeTo(RangeTo<Duration>),
    Range(Range<Duration>),
    Iter(Vec<Duration>),
}

pub trait IntoBufferSizeRange {
    fn into_buffer_size_range(self) -> BufferSizeRange;
}

impl IntoBufferSizeRange for RangeFrom<Duration> {
    fn into_buffer_size_range(self) -> BufferSizeRange {
        BufferSizeRange::RangeFrom(self)
    }
}
impl IntoBufferSizeRange for std::ops::Range<Duration> {
    fn into_buffer_size_range(self) -> BufferSizeRange {
        BufferSizeRange::Range(self)
    }
}
impl IntoBufferSizeRange for std::ops::RangeTo<Duration> {
    fn into_buffer_size_range(self) -> BufferSizeRange {
        BufferSizeRange::RangeTo(self)
    }
}
impl<const N: usize> IntoBufferSizeRange for [Duration; N] {
    fn into_buffer_size_range(self) -> BufferSizeRange {
        BufferSizeRange::Iter(self.to_vec())
    }
}

impl IntoBufferSizeRange for Vec<Duration> {
    fn into_buffer_size_range(self) -> BufferSizeRange {
        BufferSizeRange::Iter(self)
    }
}
impl IntoBufferSizeRange for Duration {
    fn into_buffer_size_range(self) -> BufferSizeRange {
        BufferSizeRange::Iter(vec![self])
    }
}
