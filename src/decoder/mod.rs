use std::io::{Read, Seek};
use std::sync::Arc;
use std::sync::Mutex;

use cpal::Endpoint;

mod vorbis;
mod wav;

/// Trait for objects that produce an audio stream.
pub trait Decoder {
    /// Appends 17ms of data to the voice.
    ///
    /// Returns false if the sound is over.
    fn write(&mut self) -> bool;

    /// Changes the volume of the sound.
    fn set_volume(&mut self, f32);

    /// Returns the total duration of the second in milliseconds.
    fn get_total_duration_ms(&self) -> u32;

    /// Returns the number of milliseconds before the end of the sound.
    fn get_remaining_duration_ms(&self) -> u32;
}

/// Builds a new `Decoder` from a data stream by determining the correct format.
pub fn decode<R>(endpoint: &Endpoint, data: R) -> Arc<Mutex<Decoder + Send>>
                 where R: Read + Seek + Send + 'static
{
    let data = match wav::WavDecoder::new(endpoint, data) {
        Err(data) => data,
        Ok(decoder) => {
            return Arc::new(Mutex::new(decoder));
        }
    };

    if let Ok(decoder) = vorbis::VorbisDecoder::new(endpoint, data) {
        return Arc::new(Mutex::new(decoder));
    }

    panic!("Invalid format");
}
