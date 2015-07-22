use std::io::Read;
use cpal::Voice;

mod wav;

pub trait Decoder {
    fn write(&mut self, &mut Voice);
}

pub fn decode<R>(data: R) -> Box<Decoder + Send> where R: Read + Send + 'static {
    if let Ok(decoder) = wav::WavDecoder::new(data) {
        return Box::new(decoder);
    }

    panic!("Invalid format");
}
