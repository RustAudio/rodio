use std::io::{Read, Seek};
use cpal::Voice;

mod vorbis;
mod wav;

/// Trait for objects that produce an audio stream.
pub trait Decoder {
    /// Appends data to the voice.
    fn write(&mut self, &mut Voice);
}

/// Builds a new `Decoder` from a data stream by determining the correct format.
pub fn decode<R>(data: R) -> Box<Decoder + Send> where R: Read + Seek + Send + 'static {
    let data = match wav::WavDecoder::new(data) {
        Err(data) => data,
        Ok(decoder) => {
            return Box::new(decoder);
        }
    };

    if let Ok(decoder) = vorbis::VorbisDecoder::new(data) {
        return Box::new(decoder);
    }

    panic!("Invalid format");
}
