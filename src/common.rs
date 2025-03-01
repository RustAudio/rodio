use std::num::NonZero;

/// Stream sample rate (a frame rate or samples per second per channel).
pub type SampleRate = NonZero<u32>;

/// Number of channels in a stream. Can never be Zero
pub type ChannelCount = NonZero<u16>;

/// Represents value of a single sample.
/// Silence corresponds to the value `0.0`. The expected amplitude range is  -1.0...1.0.
/// Values below and above this range are clipped in conversion to other sample types.
/// Use conversion traits from [dasp_sample] crate or [crate::conversions::SampleTypeConverter]
/// to convert between sample types if necessary.
pub type Sample = f32;
