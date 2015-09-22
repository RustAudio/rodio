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

    // note that it is important to do `buffer.zip(samples)` instead of `samples.zip(buffer)`
    // otherwise when the buffer's iterator is exhausted the value obtained from `samples` is
    // discarded

    match output {
        &mut UnknownTypeBuffer::U16(ref mut buffer) => {
            for (o, i) in buffer.iter_mut().zip(samples) {
                *o = i.to_u16();
            }
        },

        &mut UnknownTypeBuffer::I16(ref mut buffer) => {
            for (o, i) in buffer.iter_mut().zip(samples) {
                *o = i.to_i16();
            }
        },

        &mut UnknownTypeBuffer::F32(ref mut buffer) => {
            for (o, i) in buffer.iter_mut().zip(samples) {
                *o = i.to_f32();
            }
        },
    }
}
