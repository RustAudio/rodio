//! Decodes samples from an audio file.

#[cfg(all(feature = "symphonia-mp3", feature = "mp3"))]
compile_error!("Both symphonia-mp3 and mp3 are enabled. 
If you enabled symphonia-mp3 or symphonia-all, make sure to set default-features=false to disable the default mp3 decoder");

#[cfg(all(feature = "symphonia-wav", feature = "wav"))]
compile_error!("Both symphonia-wav and wav are enabled. 
If you enabled symphonia-wav or symphonia-all, make sure to set default-features=false to disable the default wav decoder");

#[cfg(all(feature = "symphonia-flac", feature = "flac"))]
compile_error!("Both symphonia-flac and flac are enabled. 
If you enabled symphonia-flac or symphonia-all, make sure to set default-features=false to disable the default flac decoder");

use std::fmt;
#[allow(unused_imports)]
use std::io::{Read, Seek, SeekFrom};
use std::mem;
use std::str::FromStr;
use std::time::Duration;
use thiserror::Error;

use crate::Source;

#[cfg(symphonia)]
use self::read_seek_source::ReadSeekSource;
#[cfg(symphonia)]
use ::symphonia::core::io::{MediaSource, MediaSourceStream};

#[cfg(feature = "flac")]
mod flac;
#[cfg(feature = "mp3")]
mod mp3;
#[cfg(symphonia)]
mod read_seek_source;
#[cfg(symphonia)]
mod symphonia;
#[cfg(feature = "vorbis")]
mod vorbis;
#[cfg(feature = "wav")]
mod wav;

/// Source of audio samples from decoding a file.
///
/// Supports MP3, WAV, Vorbis and Flac.
pub struct Decoder<R>(DecoderImpl<R>)
where
    R: Read + Seek;

pub struct LoopedDecoder<R>(DecoderImpl<R>)
where
    R: Read + Seek;

enum DecoderImpl<R>
where
    R: Read + Seek,
{
    #[cfg(feature = "wav")]
    Wav(wav::WavDecoder<R>),
    #[cfg(feature = "vorbis")]
    Vorbis(vorbis::VorbisDecoder<R>),
    #[cfg(feature = "flac")]
    Flac(flac::FlacDecoder<R>),
    #[cfg(feature = "mp3")]
    Mp3(mp3::Mp3Decoder<R>),
    #[cfg(symphonia)]
    Symphonia(symphonia::SymphoniaDecoder),
    None(::std::marker::PhantomData<R>),
}

impl<R> Decoder<R>
where
    R: Read + Seek + Send + 'static,
{
    /// Builds a new decoder.
    ///
    /// Attempts to automatically detect the format of the source of data.
    #[allow(unused_variables)]
    pub fn new(data: R) -> Result<Decoder<R>, DecoderError> {
        #[cfg(feature = "wav")]
        let data = match wav::WavDecoder::new(data) {
            Err(data) => data,
            Ok(decoder) => {
                return Ok(Decoder(DecoderImpl::Wav(decoder)));
            }
        };

        #[cfg(feature = "flac")]
        let data = match flac::FlacDecoder::new(data) {
            Err(data) => data,
            Ok(decoder) => {
                return Ok(Decoder(DecoderImpl::Flac(decoder)));
            }
        };

        #[cfg(feature = "vorbis")]
        let data = match vorbis::VorbisDecoder::new(data) {
            Err(data) => data,
            Ok(decoder) => {
                return Ok(Decoder(DecoderImpl::Vorbis(decoder)));
            }
        };

        #[cfg(feature = "mp3")]
        let data = match mp3::Mp3Decoder::new(data) {
            Err(data) => data,
            Ok(decoder) => {
                return Ok(Decoder(DecoderImpl::Mp3(decoder)));
            }
        };

        #[cfg(symphonia)]
        {
            let mss = MediaSourceStream::new(
                Box::new(ReadSeekSource::new(data)) as Box<dyn MediaSource>,
                Default::default(),
            );

            return match symphonia::SymphoniaDecoder::new(mss, None) {
                Err(e) => Err(e),
                Ok(decoder) => {
                    return Ok(Decoder(DecoderImpl::Symphonia(decoder)));
                }
            };
        };
        #[cfg(not(symphonia))]
        Err(DecoderError::UnrecognizedFormat)
    }
    pub fn new_looped(data: R) -> Result<LoopedDecoder<R>, DecoderError> {
        Self::new(data).map(LoopedDecoder::new)
    }

    /// Builds a new decoder from wav data.
    #[cfg(feature = "wav")]
    pub fn new_wav(data: R) -> Result<Decoder<R>, DecoderError> {
        match wav::WavDecoder::new(data) {
            Err(_) => Err(DecoderError::UnrecognizedFormat),
            Ok(decoder) => Ok(Decoder(DecoderImpl::Wav(decoder))),
        }
    }

    /// Builds a new decoder from wav data.
    #[cfg(feature = "symphonia-wav")]
    pub fn new_wav(data: R) -> Result<Decoder<R>, DecoderError> {
        Decoder::new_symphonia(data, "wav")
    }

    /// Builds a new decoder from flac data.
    #[cfg(feature = "flac")]
    pub fn new_flac(data: R) -> Result<Decoder<R>, DecoderError> {
        match flac::FlacDecoder::new(data) {
            Err(_) => Err(DecoderError::UnrecognizedFormat),
            Ok(decoder) => Ok(Decoder(DecoderImpl::Flac(decoder))),
        }
    }

    /// Builds a new decoder from flac data.
    #[cfg(feature = "symphonia-flac")]
    pub fn new_flac(data: R) -> Result<Decoder<R>, DecoderError> {
        Decoder::new_symphonia(data, "flac")
    }

    /// Builds a new decoder from vorbis data.
    #[cfg(feature = "vorbis")]
    pub fn new_vorbis(data: R) -> Result<Decoder<R>, DecoderError> {
        match vorbis::VorbisDecoder::new(data) {
            Err(_) => Err(DecoderError::UnrecognizedFormat),
            Ok(decoder) => Ok(Decoder(DecoderImpl::Vorbis(decoder))),
        }
    }

    /// Builds a new decoder from mp3 data.
    #[cfg(feature = "mp3")]
    pub fn new_mp3(data: R) -> Result<Decoder<R>, DecoderError> {
        match mp3::Mp3Decoder::new(data) {
            Err(_) => Err(DecoderError::UnrecognizedFormat),
            Ok(decoder) => Ok(Decoder(DecoderImpl::Mp3(decoder))),
        }
    }

    /// Builds a new decoder from mp3 data.
    #[cfg(feature = "symphonia-mp3")]
    pub fn new_mp3(data: R) -> Result<Decoder<R>, DecoderError> {
        Decoder::new_symphonia(data, "mp3")
    }

    /// Builds a new decoder from aac data.
    #[cfg(feature = "symphonia-aac")]
    pub fn new_aac(data: R) -> Result<Decoder<R>, DecoderError> {
        Decoder::new_symphonia(data, "aac")
    }

    /// Builds a new decoder from mp4 data.
    #[cfg(feature = "symphonia-isomp4")]
    pub fn new_mp4(data: R, hint: Mp4Type) -> Result<Decoder<R>, DecoderError> {
        Decoder::new_symphonia(data, &hint.to_string())
    }

    #[cfg(symphonia)]
    fn new_symphonia(data: R, hint: &str) -> Result<Decoder<R>, DecoderError> {
        let mss = MediaSourceStream::new(
            Box::new(ReadSeekSource::new(data)) as Box<dyn MediaSource>,
            Default::default(),
        );

        match symphonia::SymphoniaDecoder::new(mss, Some(hint)) {
            Err(e) => Err(e),
            Ok(decoder) => {
                return Ok(Decoder(DecoderImpl::Symphonia(decoder)));
            }
        }
    }
}

#[derive(Debug)]
pub enum Mp4Type {
    Mp4,
    M4a,
    M4p,
    M4b,
    M4r,
    M4v,
    Mov,
}

impl FromStr for Mp4Type {
    type Err = String;

    fn from_str(input: &str) -> Result<Mp4Type, Self::Err> {
        match &input.to_lowercase()[..] {
            "mp4" => Ok(Mp4Type::Mp4),
            "m4a" => Ok(Mp4Type::M4a),
            "m4p" => Ok(Mp4Type::M4p),
            "m4b" => Ok(Mp4Type::M4b),
            "m4r" => Ok(Mp4Type::M4r),
            "m4v" => Ok(Mp4Type::M4v),
            "mov" => Ok(Mp4Type::Mov),
            _ => Err(format!("{} is not a valid mp4 extension", input)),
        }
    }
}

impl fmt::Display for Mp4Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let res = self.to_string().to_lowercase();
        write!(f, "{}", res)
    }
}

impl<R> LoopedDecoder<R>
where
    R: Read + Seek + Send,
{
    fn new(decoder: Decoder<R>) -> LoopedDecoder<R> {
        Self(decoder.0)
    }
}

impl<R> Iterator for Decoder<R>
where
    R: Read + Seek,
{
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        match &mut self.0 {
            #[cfg(feature = "wav")]
            DecoderImpl::Wav(source) => source.next(),
            #[cfg(feature = "vorbis")]
            DecoderImpl::Vorbis(source) => source.next(),
            #[cfg(feature = "flac")]
            DecoderImpl::Flac(source) => source.next(),
            #[cfg(feature = "mp3")]
            DecoderImpl::Mp3(source) => source.next(),
            #[cfg(symphonia)]
            DecoderImpl::Symphonia(source) => source.next(),
            DecoderImpl::None(_) => None,
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.0 {
            #[cfg(feature = "wav")]
            DecoderImpl::Wav(source) => source.size_hint(),
            #[cfg(feature = "vorbis")]
            DecoderImpl::Vorbis(source) => source.size_hint(),
            #[cfg(feature = "flac")]
            DecoderImpl::Flac(source) => source.size_hint(),
            #[cfg(feature = "mp3")]
            DecoderImpl::Mp3(source) => source.size_hint(),
            #[cfg(symphonia)]
            DecoderImpl::Symphonia(source) => source.size_hint(),
            DecoderImpl::None(_) => (0, None),
        }
    }
}

impl<R> Source for Decoder<R>
where
    R: Read + Seek,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        match &self.0 {
            #[cfg(feature = "wav")]
            DecoderImpl::Wav(source) => source.current_frame_len(),
            #[cfg(feature = "vorbis")]
            DecoderImpl::Vorbis(source) => source.current_frame_len(),
            #[cfg(feature = "flac")]
            DecoderImpl::Flac(source) => source.current_frame_len(),
            #[cfg(feature = "mp3")]
            DecoderImpl::Mp3(source) => source.current_frame_len(),
            #[cfg(symphonia)]
            DecoderImpl::Symphonia(source) => source.current_frame_len(),
            DecoderImpl::None(_) => Some(0),
        }
    }

    #[inline]
    fn channels(&self) -> u16 {
        match &self.0 {
            #[cfg(feature = "wav")]
            DecoderImpl::Wav(source) => source.channels(),
            #[cfg(feature = "vorbis")]
            DecoderImpl::Vorbis(source) => source.channels(),
            #[cfg(feature = "flac")]
            DecoderImpl::Flac(source) => source.channels(),
            #[cfg(feature = "mp3")]
            DecoderImpl::Mp3(source) => source.channels(),
            #[cfg(symphonia)]
            DecoderImpl::Symphonia(source) => source.channels(),
            DecoderImpl::None(_) => 0,
        }
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        match &self.0 {
            #[cfg(feature = "wav")]
            DecoderImpl::Wav(source) => source.sample_rate(),
            #[cfg(feature = "vorbis")]
            DecoderImpl::Vorbis(source) => source.sample_rate(),
            #[cfg(feature = "flac")]
            DecoderImpl::Flac(source) => source.sample_rate(),
            #[cfg(feature = "mp3")]
            DecoderImpl::Mp3(source) => source.sample_rate(),
            #[cfg(symphonia)]
            DecoderImpl::Symphonia(source) => source.sample_rate(),
            DecoderImpl::None(_) => 1,
        }
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        match &self.0 {
            #[cfg(feature = "wav")]
            DecoderImpl::Wav(source) => source.total_duration(),
            #[cfg(feature = "vorbis")]
            DecoderImpl::Vorbis(source) => source.total_duration(),
            #[cfg(feature = "flac")]
            DecoderImpl::Flac(source) => source.total_duration(),
            #[cfg(feature = "mp3")]
            DecoderImpl::Mp3(source) => source.total_duration(),
            #[cfg(symphonia)]
            DecoderImpl::Symphonia(source) => source.total_duration(),
            DecoderImpl::None(_) => Some(Duration::default()),
        }
    }
}

impl<R> Iterator for LoopedDecoder<R>
where
    R: Read + Seek,
{
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        if let Some(sample) = match &mut self.0 {
            #[cfg(feature = "wav")]
            DecoderImpl::Wav(source) => source.next(),
            #[cfg(feature = "vorbis")]
            DecoderImpl::Vorbis(source) => source.next(),
            #[cfg(feature = "flac")]
            DecoderImpl::Flac(source) => source.next(),
            #[cfg(feature = "mp3")]
            DecoderImpl::Mp3(source) => source.next(),
            #[cfg(symphonia)]
            DecoderImpl::Symphonia(source) => source.next(),
            DecoderImpl::None(_) => None,
        } {
            Some(sample)
        } else {
            let decoder = mem::replace(&mut self.0, DecoderImpl::None(Default::default()));
            let (decoder, sample) = match decoder {
                #[cfg(feature = "wav")]
                DecoderImpl::Wav(source) => {
                    let mut reader = source.into_inner();
                    reader.seek(SeekFrom::Start(0)).ok()?;
                    let mut source = wav::WavDecoder::new(reader).ok()?;
                    let sample = source.next();
                    (DecoderImpl::Wav(source), sample)
                }
                #[cfg(feature = "vorbis")]
                DecoderImpl::Vorbis(source) => {
                    use lewton::inside_ogg::OggStreamReader;
                    let mut reader = source.into_inner().into_inner();
                    reader.seek_bytes(SeekFrom::Start(0)).ok()?;
                    let mut source = vorbis::VorbisDecoder::from_stream_reader(
                        OggStreamReader::from_ogg_reader(reader).ok()?,
                    );
                    let sample = source.next();
                    (DecoderImpl::Vorbis(source), sample)
                }
                #[cfg(feature = "flac")]
                DecoderImpl::Flac(source) => {
                    let mut reader = source.into_inner();
                    reader.seek(SeekFrom::Start(0)).ok()?;
                    let mut source = flac::FlacDecoder::new(reader).ok()?;
                    let sample = source.next();
                    (DecoderImpl::Flac(source), sample)
                }
                #[cfg(feature = "mp3")]
                DecoderImpl::Mp3(source) => {
                    let mut reader = source.into_inner();
                    reader.seek(SeekFrom::Start(0)).ok()?;
                    let mut source = mp3::Mp3Decoder::new(reader).ok()?;
                    let sample = source.next();
                    (DecoderImpl::Mp3(source), sample)
                }
                #[cfg(symphonia)]
                DecoderImpl::Symphonia(source) => {
                    let mut reader = Box::new(source).into_inner();
                    reader.seek(SeekFrom::Start(0)).ok()?;
                    let mut source = symphonia::SymphoniaDecoder::new(reader, None).ok()?;
                    let sample = source.next();
                    (DecoderImpl::Symphonia(source), sample)
                }
                none @ DecoderImpl::None(_) => (none, None),
            };
            self.0 = decoder;
            sample
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.0 {
            #[cfg(feature = "wav")]
            DecoderImpl::Wav(source) => (source.size_hint().0, None),
            #[cfg(feature = "vorbis")]
            DecoderImpl::Vorbis(source) => (source.size_hint().0, None),
            #[cfg(feature = "flac")]
            DecoderImpl::Flac(source) => (source.size_hint().0, None),
            #[cfg(feature = "mp3")]
            DecoderImpl::Mp3(source) => (source.size_hint().0, None),
            #[cfg(symphonia)]
            DecoderImpl::Symphonia(source) => (source.size_hint().0, None),
            DecoderImpl::None(_) => (0, None),
        }
    }
}

impl<R> Source for LoopedDecoder<R>
where
    R: Read + Seek,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        match &self.0 {
            #[cfg(feature = "wav")]
            DecoderImpl::Wav(source) => source.current_frame_len(),
            #[cfg(feature = "vorbis")]
            DecoderImpl::Vorbis(source) => source.current_frame_len(),
            #[cfg(feature = "flac")]
            DecoderImpl::Flac(source) => source.current_frame_len(),
            #[cfg(feature = "mp3")]
            DecoderImpl::Mp3(source) => source.current_frame_len(),
            #[cfg(symphonia)]
            DecoderImpl::Symphonia(source) => source.current_frame_len(),
            DecoderImpl::None(_) => Some(0),
        }
    }

    #[inline]
    fn channels(&self) -> u16 {
        match &self.0 {
            #[cfg(feature = "wav")]
            DecoderImpl::Wav(source) => source.channels(),
            #[cfg(feature = "vorbis")]
            DecoderImpl::Vorbis(source) => source.channels(),
            #[cfg(feature = "flac")]
            DecoderImpl::Flac(source) => source.channels(),
            #[cfg(feature = "mp3")]
            DecoderImpl::Mp3(source) => source.channels(),
            #[cfg(symphonia)]
            DecoderImpl::Symphonia(source) => source.channels(),
            DecoderImpl::None(_) => 0,
        }
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        match &self.0 {
            #[cfg(feature = "wav")]
            DecoderImpl::Wav(source) => source.sample_rate(),
            #[cfg(feature = "vorbis")]
            DecoderImpl::Vorbis(source) => source.sample_rate(),
            #[cfg(feature = "flac")]
            DecoderImpl::Flac(source) => source.sample_rate(),
            #[cfg(feature = "mp3")]
            DecoderImpl::Mp3(source) => source.sample_rate(),
            #[cfg(symphonia)]
            DecoderImpl::Symphonia(source) => source.sample_rate(),
            DecoderImpl::None(_) => 1,
        }
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// Error that can happen when creating a decoder.
#[derive(Debug, Error)]
pub enum DecoderError {
    /// The format of the data has not been recognized.
    #[error("Unrecognized format")]
    UnrecognizedFormat,

    /// The underlying Symphonia decoder returned an error
    #[error(transparent)]
    #[cfg(symphonia)]
    IoError(#[from] std::io::Error),

    #[error("Decode Error: {0}")]
    #[cfg(symphonia)]
    DecoderError(&'static str),

    #[error("Limit Error: {0}")]
    #[cfg(symphonia)]
    LimitError(&'static str),

    #[cfg(symphonia)]
    #[error("The demuxer or decoder needs to be reset before continuing")]
    ResetRequired,

    /// No streams were found by the decoder
    #[error("No streams found")]
    #[cfg(symphonia)]
    NoStreams,
}
