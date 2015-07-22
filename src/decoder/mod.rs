use std::io::Read;
use cpal::Voice;

mod wav;

/// Trait for objects that produce an audio stream.
pub trait Decoder {
    /// Appends data to the voice.
    fn write(&mut self, &mut Voice);
}

/// Builds a new `Decoder` from a data stream by determining the correct format.
pub fn decode<R>(data: R) -> Box<Decoder + Send> where R: Read + Send + 'static {
    if let Ok(decoder) = wav::WavDecoder::new(data) {
        return Box::new(decoder);
    }

    panic!("Invalid format");
}
