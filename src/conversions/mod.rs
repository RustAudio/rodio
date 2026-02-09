//! This module contains functions that convert from one PCM format to another.

pub use self::channels::ChannelCountConverter;
pub use self::sample::SampleTypeConverter;
#[allow(deprecated)]
pub use self::sample_rate::SampleRateConverter;

mod channels;
mod sample;
mod sample_rate;
