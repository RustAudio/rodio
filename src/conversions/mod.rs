/*!
This module contains function that will convert from one PCM format to another.

This includes conversion between sample formats, channels or sample rates.

*/

pub use channels::ChannelCountConverter;
pub use sample::DataConverter;
pub use sample::Sample;

mod channels;
mod sample;
pub(crate) mod sample_rate;
