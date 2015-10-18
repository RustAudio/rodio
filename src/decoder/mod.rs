use std::io::{Read, Seek};
use std::time::Duration;

use Sample;
use Source;

mod mp3;
mod vorbis;
mod wav;

/// Source of audio samples from decoding a file.
///
/// Supports WAV, MP3 and Vorbis.
pub struct Decoder<R>(DecoderImpl<R>) where R: Read + Seek;

enum DecoderImpl<R> where R: Read + Seek {
    Wav(wav::WavDecoder<R>),
    Mp3(mp3::Mp3Decoder<R>),
    Vorbis(vorbis::VorbisDecoder<R>),
}

impl<R> Decoder<R> where R: Read + Seek {
    pub fn new(data: R) -> Decoder<R> {
        let data = match wav::WavDecoder::new(data) {
            Err(data) => data,
            Ok(decoder) => {
                return Decoder(DecoderImpl::Wav(decoder));
            }
        };

        let data = match vorbis::VorbisDecoder::new(data) {
            Err(data) => data,
            Ok(decoder) => {
                return Decoder(DecoderImpl::Vorbis(decoder));
            }
        };

        let _data = match mp3::Mp3Decoder::new(data) {
            Err(data) => data,
            Ok(decoder) => {
                return Decoder(DecoderImpl::Mp3(decoder));
            }
        };

        panic!("Invalid format");
    }
}

impl<R> Iterator for Decoder<R> where R: Read + Seek {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        match self.0 {
            DecoderImpl::Wav(ref mut source) => source.next().map(|s| s.to_f32()),
            DecoderImpl::Mp3(ref mut source) => source.next().map(|s| s.to_f32()),
            DecoderImpl::Vorbis(ref mut source) => source.next().map(|s| s.to_f32()),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.0 {
            DecoderImpl::Wav(ref source) => source.size_hint(),
            DecoderImpl::Mp3(ref source) => source.size_hint(),
            DecoderImpl::Vorbis(ref source) => source.size_hint(),
        }
    }
}

impl<R> Source for Decoder<R> where R: Read + Seek {
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        match self.0 {
            DecoderImpl::Wav(ref source) => source.get_current_frame_len(),
            DecoderImpl::Mp3(ref source) => source.get_current_frame_len(),
            DecoderImpl::Vorbis(ref source) => source.get_current_frame_len(),
        }
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        match self.0 {
            DecoderImpl::Wav(ref source) => source.get_channels(),
            DecoderImpl::Mp3(ref source) => source.get_channels(),
            DecoderImpl::Vorbis(ref source) => source.get_channels(),
        }
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        match self.0 {
            DecoderImpl::Wav(ref source) => source.get_samples_rate(),
            DecoderImpl::Mp3(ref source) => source.get_samples_rate(),
            DecoderImpl::Vorbis(ref source) => source.get_samples_rate(),
        }
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        match self.0 {
            DecoderImpl::Wav(ref source) => source.get_total_duration(),
            DecoderImpl::Mp3(ref source) => source.get_total_duration(),
            DecoderImpl::Vorbis(ref source) => source.get_total_duration(),
        }
    }
}
