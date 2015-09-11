use std::io::{Read, Seek};

use cpal::Endpoint;
use cpal::Voice;

mod conversions;
mod vorbis;
mod wav;

/// Trait for objects that produce an audio stream.
pub trait Decoder {
    /// Appends data to the voice. Returns the number of nanoseconds after which new data will need
    /// to have been submitted.
    fn write(&mut self) -> u64;

    /// Changes the volume of the sound.
    fn set_volume(&mut self, f32);
}

/// Builds a new `Decoder` from a data stream by determining the correct format.
pub fn decode<R>(endpoint: &Endpoint, data: R) -> Box<Decoder + Send>
                 where R: Read + Seek + Send + 'static
{
    let data = match wav::WavDecoder::new(endpoint, data) {
        Err(data) => data,
        Ok(decoder) => {
            return Box::new(decoder);
        }
    };

    if let Ok(decoder) = vorbis::VorbisDecoder::new(endpoint, data) {
        return Box::new(decoder);
    }

    panic!("Invalid format");
}
