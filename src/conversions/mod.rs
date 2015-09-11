/*!
This module contains function that will convert from one PCM format to another.

This includes conversion between samples formats, channels or sample rates.

*/
use std::iter;
use cpal::UnknownTypeBuffer;

pub use self::sample::Sample;
pub use self::channels::ChannelsCountConverter;
pub use self::samples_rate::SamplesRateConverter;
pub use self::amplifier::AmplifierIterator;

mod amplifier;
mod channels;
mod sample;
mod samples_rate;

///
pub fn convert_and_write<I, S>(samples: I, output: &mut UnknownTypeBuffer)
                               where I: Iterator<Item=S>, S: Sample
{
    let samples = samples.chain(iter::repeat(Sample::zero_value()));

    match output {
        &mut UnknownTypeBuffer::U16(ref mut buffer) => {
            for (i, o) in samples.zip(buffer.iter_mut()) {
                *o = i.to_u16();
            }
        },

        &mut UnknownTypeBuffer::I16(ref mut buffer) => {
            for (i, o) in samples.zip(buffer.iter_mut()) {
                *o = i.to_i16();
            }
        },

        &mut UnknownTypeBuffer::F32(ref mut buffer) => {
            for (i, o) in samples.zip(buffer.iter_mut()) {
                *o = i.to_f32();
            }
        },
    }
}
