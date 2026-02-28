use std::num::NonZero;
use std::time::Duration;

use crate::{math::nz, stream::DeviceSinkConfig, ChannelCount, SampleRate};

/// The size of the buffer used by the OS
#[derive(Debug, Copy, Clone)]
pub enum BufferSize {
    /// Make the the buffer size such that is holds this duration of audio
    Duration(Duration),
    /// Make the buffer size so that it holds this many frames
    FrameCount(u32),
}

impl Default for BufferSize {
    fn default() -> Self {
        Self::Duration(Duration::from_millis(50))
    }
}

impl BufferSize {
    pub(crate) fn frame_count(&self, sample_rate: SampleRate) -> u32 {
        match self {
            BufferSize::Duration(duration) => {
                (duration.as_secs_f64() * sample_rate.get() as f64) as u32
            }
            BufferSize::FrameCount(frames) => *frames,
        }
    }
}

/// Describes the output stream's configuration
#[derive(Copy, Clone, Debug)]
pub struct OutputConfig {
    /// The number of channels
    pub channel_count: ChannelCount,
    /// The sample rate the audio card will be playing back at
    pub sample_rate: SampleRate,
    /// The buffer size, see a thorough explanation in SpeakerBuilder::with_buffer_size
    pub buffer_size: BufferSize,
    /// The sample format used by the audio card.
    /// Note we will always convert to this from f32
    pub sample_format: cpal::SampleFormat,
}
impl OutputConfig {
    fn buffer_size_frames(&self) -> u32 {
        self.buffer_size.frame_count(self.sample_rate)
    }

    pub(crate) fn supported_given(&self, supported: &cpal::SupportedStreamConfigRange) -> bool {
        let buffer_ok = match supported.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max } => {
                (min..=max).contains(&&self.buffer_size_frames())
            }
            cpal::SupportedBufferSize::Unknown => true,
        };

        buffer_ok
            && self.channel_count.get() == supported.channels()
            && self.sample_format == supported.sample_format()
            && self.sample_rate.get() <= supported.max_sample_rate()
            && self.sample_rate.get() >= supported.min_sample_rate()
    }

    pub(crate) fn with_f32_samples(&self) -> Self {
        let mut this = *self;
        this.sample_format = cpal::SampleFormat::F32;
        this
    }

    pub(crate) fn into_cpal_config(self) -> crate::stream::DeviceSinkConfig {
        DeviceSinkConfig {
            channel_count: self.channel_count,
            sample_rate: self.sample_rate,
            buffer_size: cpal::BufferSize::Fixed(self.buffer_size_frames()),
            sample_format: self.sample_format,
        }
    }
}

impl From<cpal::SupportedStreamConfig> for OutputConfig {
    fn from(value: cpal::SupportedStreamConfig) -> Self {
        use cpal::SupportedBufferSize as B;

        let sample_rate =
            NonZero::new(value.sample_rate()).expect("A supported config produces samples");
        let default_frames = BufferSize::default().frame_count(sample_rate);
        let buffer_size = match value.buffer_size() {
            B::Range { min, max } if (min..=max).contains(&&default_frames) => {
                BufferSize::default()
            }
            B::Unknown => BufferSize::default(),
            B::Range { min, .. } if default_frames < *min => BufferSize::FrameCount(*min),
            // default_frames > max
            B::Range { max, .. } => BufferSize::FrameCount(*max),
        };
        Self {
            channel_count: NonZero::new(value.channels())
                .expect("A supported config never has 0 channels"),
            sample_rate,
            buffer_size,
            sample_format: value.sample_format(),
        }
    }
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            channel_count: nz!(1),
            sample_rate: nz!(44_100),
            buffer_size: BufferSize::default(),
            sample_format: cpal::SampleFormat::F32,
        }
    }
}
