use std::io::{Read, Seek};

use Sample;
use Source;

pub mod vorbis;
pub mod wav;

/// Source of audio samples from decoding a file.
pub enum Decoder<R> where R: Read + Seek {
    Wav(wav::WavDecoder<R>),
    Vorbis(vorbis::VorbisDecoder),
}

impl<R> Decoder<R> where R: Read + Seek + Send + 'static {
    pub fn new(data: R) -> Decoder<R> {
        let data = match wav::WavDecoder::new(data) {
            Err(data) => data,
            Ok(decoder) => {
                return Decoder::Wav(decoder);
            }
        };

        if let Ok(decoder) = vorbis::VorbisDecoder::new(data) {
            return Decoder::Vorbis(decoder);
        }

        panic!("Invalid format");
    }
}

impl<R> Iterator for Decoder<R> where R: Read + Seek {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        match self {
            &mut Decoder::Wav(ref mut source) => source.next().map(|s| s.to_f32()),
            &mut Decoder::Vorbis(ref mut source) => source.next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            &Decoder::Wav(ref source) => source.size_hint(),
            &Decoder::Vorbis(ref source) => source.size_hint(),
        }
    }
}

// TODO: ExactSizeIterator

impl<R> Source for Decoder<R> where R: Read + Seek {
    #[inline]
    fn get_current_frame_len(&self) -> usize {
        match self {
            &Decoder::Wav(ref source) => source.get_current_frame_len(),
            &Decoder::Vorbis(ref source) => source.get_current_frame_len(),
        }
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        match self {
            &Decoder::Wav(ref source) => source.get_channels(),
            &Decoder::Vorbis(ref source) => source.get_channels(),
        }
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        match self {
            &Decoder::Wav(ref source) => source.get_samples_rate(),
            &Decoder::Vorbis(ref source) => source.get_samples_rate(),
        }
    }
}
