use std::io::{Read, Seek};
use std::sync::Arc;
use std::sync::Mutex;

mod vorbis;
mod wav;

/// Trait for objects that produce an audio stream.
pub trait Decoder: Iterator /*+ ExactSizeIterator*/ {       // TODO: should be exact size, but not enforced yet
    /// Returns the total duration of the second in milliseconds.
    fn get_total_duration_ms(&self) -> u32;
}

/// Builds a new `Decoder` from a data stream by determining the correct format.
pub fn decode<R>(data: R, output_channels: u16, output_samples_rate: u32)
                 -> Arc<Mutex<Decoder<Item=f32> + Send>>
                 where R: Read + Seek + Send + 'static
{
    let data = match wav::WavDecoder::new(data, output_channels, output_samples_rate) {
        Err(data) => data,
        Ok(decoder) => {
            return Arc::new(Mutex::new(decoder));
        }
    };

    if let Ok(decoder) = vorbis::VorbisDecoder::new(data, output_channels, output_samples_rate) {
        return Arc::new(Mutex::new(decoder));
    }

    panic!("Invalid format");
}
