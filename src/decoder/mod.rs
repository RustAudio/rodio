//! Decodes audio samples from various audio file formats.
//!
//! This module provides decoders for common audio formats like MP3, WAV, Vorbis and FLAC.
//! It supports both one-shot playback and looped playback of audio files.
//!
//! # Examples
//!
//! Basic usage:
//! ```no_run
//! use std::fs::File;
//! use rodio::Decoder;
//!
//! let file = File::open("audio.mp3").unwrap();
//! let decoder = Decoder::new(file).unwrap();
//! ```
//!
//! Using the builder pattern for more control:
//! ```no_run
//! use std::fs::File;
//! use rodio::Decoder;
//!
//! let file = File::open("audio.mp3").unwrap();
//! let decoder = Decoder::builder()
//!     .with_data(file)
//!     .with_hint("mp3")
//!     .with_gapless(true)
//!     .build()
//!     .unwrap();
//! ```

use std::error::Error;
use std::fmt;
#[allow(unused_imports)]
use std::io::{Read, Seek, SeekFrom};
use std::mem;
use std::str::FromStr;
use std::time::Duration;

use crate::source::SeekError;
use crate::{Sample, Source};

#[cfg(feature = "symphonia")]
use self::read_seek_source::ReadSeekSource;
use crate::common::{ChannelCount, SampleRate};
#[cfg(feature = "symphonia")]
use ::symphonia::core::io::{MediaSource, MediaSourceStream};

#[cfg(all(feature = "flac", not(feature = "symphonia-flac")))]
mod flac;
#[cfg(all(feature = "minimp3", not(feature = "symphonia-mp3")))]
mod mp3;
#[cfg(feature = "symphonia")]
mod read_seek_source;
#[cfg(feature = "symphonia")]
/// Symphonia decoders types
pub mod symphonia;
#[cfg(all(feature = "vorbis", not(feature = "symphonia-vorbis")))]
mod vorbis;
#[cfg(all(feature = "wav", not(feature = "symphonia-wav")))]
mod wav;

/// Source of audio samples from decoding a file.
///
/// # Examples
///
/// ```no_run
/// use std::fs::File;
/// use rodio::Decoder;
///
/// let file = File::open("audio.mp3").unwrap();
/// let decoder = Decoder::new(file).unwrap();
/// ```
pub struct Decoder<R: Read + Seek>(DecoderImpl<R>);

/// Source of audio samples from decoding a file that never ends.
/// When the end of the file is reached, the decoder starts again from the beginning.
///
/// # Examples
///
/// ```no_run
/// use std::fs::File;
/// use rodio::Decoder;
///
/// let file = File::open("audio.mp3").unwrap();
/// let looped_decoder = Decoder::new_looped(file).unwrap();
/// ```
pub struct LoopedDecoder<R: Read + Seek> {
    /// The underlying decoder implementation.
    inner: DecoderImpl<R>,
    /// Configuration settings for the decoder.
    settings: Settings,
}

// Cannot really reduce the size of the VorbisDecoder. There are not any
// arrays just a lot of struct fields.
#[allow(clippy::large_enum_variant)]
enum DecoderImpl<R: Read + Seek> {
    #[cfg(all(feature = "wav", not(feature = "symphonia-wav")))]
    Wav(wav::WavDecoder<R>),
    #[cfg(all(feature = "vorbis", not(feature = "symphonia-vorbis")))]
    Vorbis(vorbis::VorbisDecoder<R>),
    #[cfg(all(feature = "flac", not(feature = "symphonia-flac")))]
    Flac(flac::FlacDecoder<R>),
    #[cfg(all(feature = "minimp3", not(feature = "symphonia-mp3")))]
    Mp3(mp3::Mp3Decoder<R>),
    #[cfg(feature = "symphonia")]
    Symphonia(symphonia::SymphoniaDecoder),
    None(::std::marker::PhantomData<R>),
}

/// Audio decoder configuration settings.
/// Support for these settings depends on the underlying decoder implementation.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The length of the stream in bytes.
    /// When known, this can be used to optimize operations like seeking and calculating durations.
    pub(crate) byte_len: Option<u64>,
    /// Whether to use coarse seeking. This needs `byte_len` to be set.
    /// Coarse seeking is faster but less accurate: it may seek to a position slightly before or
    /// after the requested one, especially when the bitrate is variable.
    pub(crate) coarse_seek: bool,
    /// Whether to trim frames for gapless playback.
    pub(crate) gapless: bool,
    /// A hint or extension for the decoder about the format of the stream.
    /// When known, this can help the decoder to select the correct demuxer.
    pub(crate) hint: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            byte_len: None,
            coarse_seek: false,
            gapless: true,
            hint: None,
        }
    }
}

/// Builder for configuring and creating a decoder.
///
/// This provides a flexible way to configure decoder settings before creating
/// the actual decoder instance.
///
/// # Examples
///
/// ```no_run
/// use std::fs::File;
/// use rodio::Decoder;
///
/// let file = File::open("audio.mp3").unwrap();
/// let decoder = Decoder::builder()
///     .with_data(file)
///     .with_hint("mp3")
///     .with_gapless(true)
///     .build()
///     .unwrap();
/// ```
#[derive(Clone, Debug)]
pub struct DecoderBuilder<R> {
    /// The input data source to decode.
    data: Option<R>,
    /// Configuration settings for the decoder.
    settings: Settings,
    /// Whether the decoder should loop playback.
    looped: bool,
}

impl<R> Default for DecoderBuilder<R> {
    fn default() -> Self {
        Self {
            data: None,
            settings: Settings::default(),
            looped: false,
        }
    }
}

/// Output type from building a decoder.
///
/// This enum represents either a normal decoder or a looped decoder,
/// depending on how the decoder was configured during building.
pub enum DecoderOutput<R>
where
    R: Read + Seek,
{
    /// A normal decoder that plays once.
    Normal(Decoder<R>),
    /// A looped decoder that repeats playback.
    Looped(LoopedDecoder<R>),
}

impl<R: Read + Seek + Send + Sync + 'static> DecoderBuilder<R> {
    /// Creates a new decoder builder with default settings.
    ///
    /// # Examples
    /// ```no_run
    /// use rodio::Decoder;
    /// use std::fs::File;
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let decoder = Decoder::builder()
    ///     .with_data(file)
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the input data source to decode.
    pub fn with_data(mut self, data: R) -> Self {
        self.data = Some(data);
        self
    }

    /// Sets the byte length of the stream.
    /// When known, this can be used to optimize operations like seeking and calculating durations.
    pub fn with_byte_len(mut self, byte_len: u64) -> Self {
        self.settings.byte_len = Some(byte_len);
        self
    }

    /// Enables or disables coarse seeking.
    ///
    /// This needs byte_len to be set. Coarse seeking is faster but less accurate:
    /// it may seek to a position slightly before or after the requested one,
    /// especially when the bitrate is variable.
    pub fn with_coarse_seek(mut self, coarse_seek: bool) -> Self {
        self.settings.coarse_seek = coarse_seek;
        self
    }

    /// Enables or disables gapless playback.
    ///
    /// When enabled, removes silence between tracks for formats that support it.
    pub fn with_gapless(mut self, gapless: bool) -> Self {
        self.settings.gapless = gapless;
        self
    }

    /// Sets a format hint for the decoder.
    ///
    /// When known, this can help the decoder to select the correct demuxer.
    /// Common values are "mp3", "wav", "flac", "ogg", etc.
    pub fn with_hint(mut self, hint: &str) -> Self {
        self.settings.hint = Some(hint.to_string());
        self
    }

    /// Configures the decoder to loop playback.
    ///
    /// When enabled, the decoder will restart from the beginning when it reaches the end.
    pub fn looped(mut self, looped: bool) -> Self {
        self.looped = looped;
        self
    }

    /// Builds the decoder with the configured settings.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined
    /// or is not supported.
    pub fn build(self) -> Result<DecoderOutput<R>, DecoderError> {
        let data = self.data.ok_or(DecoderError::UnrecognizedFormat)?;

        #[cfg(all(feature = "wav", not(feature = "symphonia-wav")))]
        let data = match wav::WavDecoder::new(data) {
            Ok(decoder) => {
                return Ok(wrap_decoder(
                    DecoderImpl::Wav(decoder),
                    self.settings,
                    self.looped,
                ))
            }
            Err(data) => data,
        };

        #[cfg(all(feature = "flac", not(feature = "symphonia-flac")))]
        let data = match flac::FlacDecoder::new(data) {
            Ok(decoder) => {
                return Ok(wrap_decoder(
                    DecoderImpl::Flac(decoder),
                    self.settings,
                    self.looped,
                ))
            }
            Err(data) => data,
        };

        #[cfg(all(feature = "vorbis", not(feature = "symphonia-vorbis")))]
        let data = match vorbis::VorbisDecoder::new(data) {
            Ok(decoder) => {
                return Ok(wrap_decoder(
                    DecoderImpl::Vorbis(decoder),
                    self.settings,
                    self.looped,
                ))
            }
            Err(data) => data,
        };

        #[cfg(all(feature = "minimp3", not(feature = "symphonia-mp3")))]
        let data = match mp3::Mp3Decoder::new(data) {
            Ok(decoder) => {
                return Ok(wrap_decoder(
                    DecoderImpl::Mp3(decoder),
                    self.settings,
                    self.looped,
                ))
            }
            Err(data) => data,
        };

        #[cfg(feature = "symphonia")]
        {
            let mss = MediaSourceStream::new(
                Box::new(ReadSeekSource::new(data, self.settings.byte_len)) as Box<dyn MediaSource>,
                Default::default(),
            );

            symphonia::SymphoniaDecoder::new(mss, &self.settings).map(|decoder| {
                wrap_decoder(DecoderImpl::Symphonia(decoder), self.settings, self.looped)
            })
        }

        #[cfg(not(feature = "symphonia"))]
        Err(DecoderError::UnrecognizedFormat)
    }
}

/// Helper function to wrap a DecoderImpl in the appropriate DecoderOutput variant
#[inline]
fn wrap_decoder<R: Read + Seek>(
    decoder: DecoderImpl<R>,
    settings: Settings,
    looped: bool,
) -> DecoderOutput<R> {
    if looped {
        DecoderOutput::Looped(LoopedDecoder {
            inner: decoder,
            settings,
        })
    } else {
        DecoderOutput::Normal(Decoder(decoder))
    }
}

impl<R: Read + Seek> DecoderImpl<R> {
    #[inline]
    fn next(&mut self) -> Option<Sample> {
        match self {
            #[cfg(all(feature = "wav", not(feature = "symphonia-wav")))]
            DecoderImpl::Wav(source) => source.next(),
            #[cfg(all(feature = "vorbis", not(feature = "symphonia-vorbis")))]
            DecoderImpl::Vorbis(source) => source.next(),
            #[cfg(all(feature = "flac", not(feature = "symphonia-flac")))]
            DecoderImpl::Flac(source) => source.next(),
            #[cfg(all(feature = "minimp3", not(feature = "symphonia-mp3")))]
            DecoderImpl::Mp3(source) => source.next(),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source) => source.next(),
            DecoderImpl::None(_) => None,
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            #[cfg(all(feature = "wav", not(feature = "symphonia-wav")))]
            DecoderImpl::Wav(source) => source.size_hint(),
            #[cfg(all(feature = "vorbis", not(feature = "symphonia-vorbis")))]
            DecoderImpl::Vorbis(source) => source.size_hint(),
            #[cfg(all(feature = "flac", not(feature = "symphonia-flac")))]
            DecoderImpl::Flac(source) => source.size_hint(),
            #[cfg(all(feature = "minimp3", not(feature = "symphonia-mp3")))]
            DecoderImpl::Mp3(source) => source.size_hint(),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source) => source.size_hint(),
            DecoderImpl::None(_) => (0, None),
        }
    }

    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        match self {
            #[cfg(all(feature = "wav", not(feature = "symphonia-wav")))]
            DecoderImpl::Wav(source) => source.current_span_len(),
            #[cfg(all(feature = "vorbis", not(feature = "symphonia-vorbis")))]
            DecoderImpl::Vorbis(source) => source.current_span_len(),
            #[cfg(all(feature = "flac", not(feature = "symphonia-flac")))]
            DecoderImpl::Flac(source) => source.current_span_len(),
            #[cfg(all(feature = "minimp3", not(feature = "symphonia-mp3")))]
            DecoderImpl::Mp3(source) => source.current_span_len(),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source) => source.current_span_len(),
            DecoderImpl::None(_) => Some(0),
        }
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        match self {
            #[cfg(all(feature = "wav", not(feature = "symphonia-wav")))]
            DecoderImpl::Wav(source) => source.channels(),
            #[cfg(all(feature = "vorbis", not(feature = "symphonia-vorbis")))]
            DecoderImpl::Vorbis(source) => source.channels(),
            #[cfg(all(feature = "flac", not(feature = "symphonia-flac")))]
            DecoderImpl::Flac(source) => source.channels(),
            #[cfg(all(feature = "minimp3", not(feature = "symphonia-mp3")))]
            DecoderImpl::Mp3(source) => source.channels(),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source) => source.channels(),
            DecoderImpl::None(_) => 0,
        }
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        match self {
            #[cfg(all(feature = "wav", not(feature = "symphonia-wav")))]
            DecoderImpl::Wav(source) => source.sample_rate(),
            #[cfg(all(feature = "vorbis", not(feature = "symphonia-vorbis")))]
            DecoderImpl::Vorbis(source) => source.sample_rate(),
            #[cfg(all(feature = "flac", not(feature = "symphonia-flac")))]
            DecoderImpl::Flac(source) => source.sample_rate(),
            #[cfg(all(feature = "minimp3", not(feature = "symphonia-mp3")))]
            DecoderImpl::Mp3(source) => source.sample_rate(),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source) => source.sample_rate(),
            DecoderImpl::None(_) => 1,
        }
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        match self {
            #[cfg(all(feature = "wav", not(feature = "symphonia-wav")))]
            DecoderImpl::Wav(source) => source.total_duration(),
            #[cfg(all(feature = "vorbis", not(feature = "symphonia-vorbis")))]
            DecoderImpl::Vorbis(source) => source.total_duration(),
            #[cfg(all(feature = "flac", not(feature = "symphonia-flac")))]
            DecoderImpl::Flac(source) => source.total_duration(),
            #[cfg(all(feature = "minimp3", not(feature = "symphonia-mp3")))]
            DecoderImpl::Mp3(source) => source.total_duration(),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source) => source.total_duration(),
            DecoderImpl::None(_) => Some(Duration::default()),
        }
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        match self {
            #[cfg(all(feature = "wav", not(feature = "symphonia-wav")))]
            DecoderImpl::Wav(source) => source.try_seek(pos),
            #[cfg(all(feature = "vorbis", not(feature = "symphonia-vorbis")))]
            DecoderImpl::Vorbis(source) => source.try_seek(pos),
            #[cfg(all(feature = "flac", not(feature = "symphonia-flac")))]
            DecoderImpl::Flac(source) => source.try_seek(pos),
            #[cfg(all(feature = "minimp3", not(feature = "symphonia-mp3")))]
            DecoderImpl::Mp3(source) => source.try_seek(pos),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source) => source.try_seek(pos),
            DecoderImpl::None(_) => Err(SeekError::NotSupported {
                underlying_source: "DecoderImpl::None",
            }),
        }
    }
}

impl<R: Read + Seek + Send + Sync + 'static> Decoder<R> {
    /// Creates a new decoder builder to configure decoder settings.
    ///
    /// # Examples
    /// ```no_run
    /// use rodio::Decoder;
    /// use std::fs::File;
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let decoder = Decoder::builder()
    ///     .with_data(file)
    ///     .with_hint("mp3")
    ///     .with_gapless(true)
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn builder() -> DecoderBuilder<R> {
        DecoderBuilder::new()
    }

    /// Builds a new decoder with default settings.
    ///
    /// Attempts to automatically detect the format of the source of data.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined
    /// or is not supported.
    pub fn new(data: R) -> Result<Self, DecoderError> {
        match Self::builder().with_data(data).build()? {
            DecoderOutput::Normal(decoder) => Ok(decoder),
            DecoderOutput::Looped(_) => unreachable!("Builder defaults to non-looped"),
        }
    }

    /// Builds a new looped decoder with default settings.
    ///
    /// Attempts to automatically detect the format of the source of data.
    /// The decoder will restart from the beginning when it reaches the end.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined
    /// or is not supported.
    pub fn new_looped(data: R) -> Result<LoopedDecoder<R>, DecoderError> {
        match Self::builder().with_data(data).looped(true).build()? {
            DecoderOutput::Looped(decoder) => Ok(decoder),
            DecoderOutput::Normal(_) => unreachable!("Builder was set to looped"),
        }
    }

    /// Builds a new decoder with WAV format hint.
    ///
    /// This method provides a hint that the data is WAV format, which may help the decoder
    /// identify the format more quickly. However, if WAV decoding fails, other formats
    /// will still be attempted.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError::UnrecognizedFormat` if no suitable decoder was found.
    ///
    /// # Examples
    /// ```no_run
    /// use rodio::Decoder;
    /// use std::fs::File;
    ///
    /// let file = File::open("audio.wav").unwrap();
    /// let decoder = Decoder::new_wav(file).unwrap();
    /// ```
    #[cfg(any(feature = "wav", feature = "symphonia-wav"))]
    pub fn new_wav(data: R) -> Result<Self, DecoderError> {
        match Self::builder().with_data(data).with_hint("wav").build()? {
            DecoderOutput::Normal(decoder) => Ok(decoder),
            DecoderOutput::Looped(_) => unreachable!(),
        }
    }

    /// Builds a new decoder with FLAC format hint.
    ///
    /// This method provides a hint that the data is FLAC format, which may help the decoder
    /// identify the format more quickly. However, if FLAC decoding fails, other formats
    /// will still be attempted.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError::UnrecognizedFormat` if no suitable decoder was found.
    ///
    /// # Examples
    /// ```no_run
    /// use rodio::Decoder;
    /// use std::fs::File;
    ///
    /// let file = File::open("audio.flac").unwrap();
    /// let decoder = Decoder::new_flac(file).unwrap();
    /// ```
    #[cfg(any(feature = "flac", feature = "symphonia-flac"))]
    pub fn new_flac(data: R) -> Result<Self, DecoderError> {
        match Self::builder().with_data(data).with_hint("flac").build()? {
            DecoderOutput::Normal(decoder) => Ok(decoder),
            DecoderOutput::Looped(_) => unreachable!(),
        }
    }

    /// Builds a new decoder with Vorbis format hint.
    ///
    /// This method provides a hint that the data is Vorbis format, which may help the decoder
    /// identify the format more quickly. However, if Vorbis decoding fails, other formats
    /// will still be attempted.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError::UnrecognizedFormat` if no suitable decoder was found.
    ///
    /// # Examples
    /// ```no_run
    /// use rodio::Decoder;
    /// use std::fs::File;
    ///
    /// let file = File::open("audio.ogg").unwrap();
    /// let decoder = Decoder::new_vorbis(file).unwrap();
    /// ```
    #[cfg(any(feature = "vorbis", feature = "symphonia-vorbis"))]
    pub fn new_vorbis(data: R) -> Result<Self, DecoderError> {
        match Self::builder().with_data(data).with_hint("ogg").build()? {
            DecoderOutput::Normal(decoder) => Ok(decoder),
            DecoderOutput::Looped(_) => unreachable!(),
        }
    }

    /// Builds a new decoder with MP3 format hint.
    ///
    /// This method provides a hint that the data is MP3 format, which may help the decoder
    /// identify the format more quickly. However, if MP3 decoding fails, other formats
    /// will still be attempted.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError::UnrecognizedFormat` if no suitable decoder was found.
    ///
    /// # Examples
    /// ```no_run
    /// use rodio::Decoder;
    /// use std::fs::File;
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let decoder = Decoder::new_mp3(file).unwrap();
    /// ```
    #[cfg(any(feature = "minimp3", feature = "symphonia-mp3"))]
    pub fn new_mp3(data: R) -> Result<Self, DecoderError> {
        match Self::builder().with_data(data).with_hint("mp3").build()? {
            DecoderOutput::Normal(decoder) => Ok(decoder),
            DecoderOutput::Looped(_) => unreachable!(),
        }
    }

    /// Builds a new decoder with AAC format hint.
    ///
    /// This method provides a hint that the data is AAC format, which may help the decoder
    /// identify the format more quickly. However, if AAC decoding fails, other formats
    /// will still be attempted.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError::UnrecognizedFormat` if no suitable decoder was found.
    ///
    /// # Examples
    /// ```no_run
    /// use rodio::Decoder;
    /// use std::fs::File;
    ///
    /// let file = File::open("audio.aac").unwrap();
    /// let decoder = Decoder::new_aac(file).unwrap();
    /// ```
    #[cfg(feature = "symphonia-aac")]
    pub fn new_aac(data: R) -> Result<Self, DecoderError> {
        match Self::builder().with_data(data).with_hint("aac").build()? {
            DecoderOutput::Normal(decoder) => Ok(decoder),
            DecoderOutput::Looped(_) => unreachable!(),
        }
    }

    /// Builds a new decoder with MP4 container format hint.
    ///
    /// This method provides a hint about the MP4 container type, which may help the decoder
    /// identify the format more quickly. However, if MP4 decoding fails, other formats
    /// will still be attempted.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError::UnrecognizedFormat` if no suitable decoder was found.
    ///
    /// # Examples
    /// ```no_run
    /// use rodio::{Decoder, Mp4Type};
    /// use std::fs::File;
    ///
    /// let file = File::open("audio.m4a").unwrap();
    /// let decoder = Decoder::new_mp4(file, Mp4Type::M4a).unwrap();
    /// ```
    #[cfg(feature = "symphonia-isomp4")]
    pub fn new_mp4(data: R, hint: Mp4Type) -> Result<Self, DecoderError> {
        match Self::builder()
            .with_data(data)
            .with_hint(&hint.to_string())
            .build()?
        {
            DecoderOutput::Normal(decoder) => Ok(decoder),
            DecoderOutput::Looped(_) => unreachable!(),
        }
    }
}

/// Type for specifying MP4 container format variants.
#[allow(missing_docs)] // Reason: will be removed, see: #612
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
            _ => Err(format!("{input} is not a valid mp4 extension")),
        }
    }
}

impl fmt::Display for Mp4Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let text = match self {
            Mp4Type::Mp4 => "mp4",
            Mp4Type::M4a => "m4a",
            Mp4Type::M4p => "m4p",
            Mp4Type::M4b => "m4b",
            Mp4Type::M4r => "m4r",
            Mp4Type::M4v => "m4v",
            Mp4Type::Mov => "mov",
        };
        write!(f, "{text}")
    }
}

impl<R> Iterator for Decoder<R>
where
    R: Read + Seek,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<R> Source for Decoder<R>
where
    R: Read + Seek,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.0.current_span_len()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.0.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.0.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.0.total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.0.try_seek(pos)
    }
}

impl<R> Iterator for LoopedDecoder<R>
where
    R: Read + Seek,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(sample) = self.inner.next() {
            Some(sample)
        } else {
            let decoder = mem::replace(&mut self.inner, DecoderImpl::None(Default::default()));
            let (decoder, sample) = match decoder {
                #[cfg(all(feature = "wav", not(feature = "symphonia-wav")))]
                DecoderImpl::Wav(source) => {
                    let mut reader = source.into_inner();
                    reader.seek(SeekFrom::Start(0)).ok()?;
                    let mut source = wav::WavDecoder::new(reader).ok()?;
                    let sample = source.next();
                    (DecoderImpl::Wav(source), sample)
                }
                #[cfg(all(feature = "vorbis", not(feature = "symphonia-vorbis")))]
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
                #[cfg(all(feature = "flac", not(feature = "symphonia-flac")))]
                DecoderImpl::Flac(source) => {
                    let mut reader = source.into_inner();
                    reader.seek(SeekFrom::Start(0)).ok()?;
                    let mut source = flac::FlacDecoder::new(reader).ok()?;
                    let sample = source.next();
                    (DecoderImpl::Flac(source), sample)
                }
                #[cfg(all(feature = "minimp3", not(feature = "symphonia-mp3")))]
                DecoderImpl::Mp3(source) => {
                    let mut reader = source.into_inner();
                    reader.seek(SeekFrom::Start(0)).ok()?;
                    let mut source = mp3::Mp3Decoder::new(reader).ok()?;
                    let sample = source.next();
                    (DecoderImpl::Mp3(source), sample)
                }
                #[cfg(feature = "symphonia")]
                DecoderImpl::Symphonia(source) => {
                    let mut reader = source.into_inner();
                    reader.seek(SeekFrom::Start(0)).ok()?;
                    let mut source =
                        symphonia::SymphoniaDecoder::new(reader, &self.settings).ok()?;
                    let sample = source.next();
                    (DecoderImpl::Symphonia(source), sample)
                }
                none @ DecoderImpl::None(_) => (none, None),
            };
            self.inner = decoder;
            sample
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<R> Source for LoopedDecoder<R>
where
    R: Read + Seek,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)
    }
}

/// Errors that can occur when creating a decoder.
#[derive(Debug, Clone)]
pub enum DecoderError {
    /// The format of the data has not been recognized.
    UnrecognizedFormat,

    /// An IO error occurred while reading, writing, or seeking the stream.
    #[cfg(feature = "symphonia")]
    IoError(String),

    /// The stream contained malformed data and could not be decoded or demuxed.
    #[cfg(feature = "symphonia")]
    DecodeError(&'static str),

    /// A default or user-defined limit was reached while decoding or demuxing the stream. Limits
    /// are used to prevent denial-of-service attacks from malicious streams.
    #[cfg(feature = "symphonia")]
    LimitError(&'static str),

    /// The demuxer or decoder needs to be reset before continuing.
    #[cfg(feature = "symphonia")]
    ResetRequired,

    /// No streams were found by the decoder
    #[cfg(feature = "symphonia")]
    NoStreams,
}

impl fmt::Display for DecoderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            DecoderError::UnrecognizedFormat => "Unrecognized format",
            #[cfg(feature = "symphonia")]
            DecoderError::IoError(msg) => &msg[..],
            #[cfg(feature = "symphonia")]
            DecoderError::DecodeError(msg) => msg,
            #[cfg(feature = "symphonia")]
            DecoderError::LimitError(msg) => msg,
            #[cfg(feature = "symphonia")]
            DecoderError::ResetRequired => "Reset required",
            #[cfg(feature = "symphonia")]
            DecoderError::NoStreams => "No streams",
        };
        write!(f, "{text}")
    }
}

impl Error for DecoderError {}
