use std::error::Error;
use std::fmt;
use std::io::{Read, Seek};
use std::time::Duration;

use Sample;
use Source;

mod vorbis;
mod wav;

/// Source of audio samples from decoding a file.
///
/// Supports WAV and Vorbis.
pub struct Decoder<R>(DecoderImpl<R>) where R: Read + Seek;

enum DecoderImpl<R>
    where R: Read + Seek
{
    Wav(wav::WavDecoder<R>),
    Vorbis(vorbis::VorbisDecoder<R>),
}

impl<R> Decoder<R>
    where R: Read + Seek + Send + 'static
{
    /// Builds a new decoder.
    ///
    /// Attempts to automatically detect the format of the source of data.
    pub fn new(data: R) -> Result<Decoder<R>, DecoderError> {
        let data = match wav::WavDecoder::new(data) {
            Err(data) => data,
            Ok(decoder) => {
                return Ok(Decoder(DecoderImpl::Wav(decoder)));
            }
        };

        if let Ok(decoder) = vorbis::VorbisDecoder::new(data) {
            return Ok(Decoder(DecoderImpl::Vorbis(decoder)));
        }

        Err(DecoderError::UnrecognizedFormat)
    }
}

impl<R> Iterator for Decoder<R>
    where R: Read + Seek
{
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        match self.0 {
            DecoderImpl::Wav(ref mut source) => source.next().map(|s| s.to_f32()),
            DecoderImpl::Vorbis(ref mut source) => source.next().map(|s| s.to_f32()),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.0 {
            DecoderImpl::Wav(ref source) => source.size_hint(),
            DecoderImpl::Vorbis(ref source) => source.size_hint(),
        }
    }
}

impl<R> Source for Decoder<R>
    where R: Read + Seek
{
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        match self.0 {
            DecoderImpl::Wav(ref source) => source.get_current_frame_len(),
            DecoderImpl::Vorbis(ref source) => source.get_current_frame_len(),
        }
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        match self.0 {
            DecoderImpl::Wav(ref source) => source.get_channels(),
            DecoderImpl::Vorbis(ref source) => source.get_channels(),
        }
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        match self.0 {
            DecoderImpl::Wav(ref source) => source.get_samples_rate(),
            DecoderImpl::Vorbis(ref source) => source.get_samples_rate(),
        }
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        match self.0 {
            DecoderImpl::Wav(ref source) => source.get_total_duration(),
            DecoderImpl::Vorbis(ref source) => source.get_total_duration(),
        }
    }
}

/// Error that can happen when creating a decoder.
#[derive(Debug, Clone)]
pub enum DecoderError {
    /// The format of the data has not been recognized.
    UnrecognizedFormat,
}

impl fmt::Display for DecoderError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &DecoderError::UnrecognizedFormat => write!(f, "Unrecognized format"),
        }
    }
}

impl Error for DecoderError {
    fn description(&self) -> &str {
        match self {
            &DecoderError::UnrecognizedFormat => "Unrecognized format",
        }
    }
}
