/*!
This module contains function that will convert from one PCM format to another.

This includes conversion between sample formats, channels or sample rates.

*/

pub use self::channels::ChannelCountConverter;
pub use self::sample::DataConverter;
pub use self::sample::Sample;
pub use self::sample_rate::SampleRateConverter;

mod channels;
mod sample;
mod sample_rate;
