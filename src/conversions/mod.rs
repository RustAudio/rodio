/*!
This module contains function that will convert from one PCM format to another.

This includes conversion between samples formats, channels or sample rates.

*/
pub use self::sample::Sample;
pub use self::sample::DataConverter;
pub use self::channels::ChannelsCountConverter;
pub use self::samples_rate::SamplesRateConverter;

mod channels;
pub mod sample;     // TODO: < shouldn't be public ; there's a bug in Rust 1.4 and below that makes this `pub` mandatory
mod samples_rate;
