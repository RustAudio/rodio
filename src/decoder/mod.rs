//! Decodes audio samples from various audio file formats.
//!
//! This module provides decoders for common audio formats like MP3, WAV, Vorbis and FLAC.
//! It supports both one-shot playback and looped playback of audio files.
//!
//! # Examples
//!
//! Recommended way to decode files (automatically sets up seeking and duration):
//! ```no_run
//! use std::fs::File;
//! use rodio::Decoder;
//!
//! let file = File::open("audio.mp3").unwrap();
//! let decoder = Decoder::try_from(file).unwrap();  // Automatically sets byte_len from metadata
//! ```
//!
//! For more control over decoder settings:
//! ```no_run
//! use std::fs::File;
//! use rodio::Decoder;
//!
//! let file = File::open("audio.mp3").unwrap();
//! let len = file.metadata().unwrap().len();
//!
//! let decoder = Decoder::builder()
//!     .with_data(file)
//!     .with_byte_len(len)      // Enable seeking and duration calculation
//!     .with_seekable(true)     // Enable seeking operations
//!     .with_hint("mp3")        // Optional format hint
//!     .build()
//!     .unwrap();
//! ```

use std::fmt;
use std::io::BufReader;
#[allow(unused_imports)]
use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;
use std::{error::Error, marker::PhantomData};

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

/// Source of audio samples decoded from an input stream.
/// See the [module-level documentation](self) for examples and usage.
pub struct Decoder<R: Read + Seek>(DecoderImpl<R>);

/// Source of audio samples from decoding a file that never ends.
/// When the end of the file is reached, the decoder starts again from the beginning.
///
/// A `LoopedDecoder` will attempt to seek back to the start of the stream when it reaches
/// the end. If seeking fails for any reason (like IO errors), iteration will stop.
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
    inner: Option<DecoderImpl<R>>,
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
    Symphonia(symphonia::SymphoniaDecoder, PhantomData<R>),
}

/// Audio decoder configuration settings.
/// Support for these settings depends on the underlying decoder implementation.
/// Currently, settings are only used by the Symphonia decoder.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The length of the stream in bytes.
    /// This is required for:
    /// - Reliable seeking operations
    /// - Duration calculations in formats that lack timing information (e.g. MP3, Vorbis)
    ///
    /// Can be obtained from file metadata or by seeking to the end of the stream.
    pub(crate) byte_len: Option<u64>,

    /// Whether to use coarse seeking.
    /// Coarse seeking is faster but less accurate: it may seek to a position slightly before or
    /// after the requested one, especially when the bitrate is variable.
    pub(crate) coarse_seek: bool,

    /// Whether to trim frames for gapless playback.
    /// Note: Disabling this may affect duration calculations for some formats
    /// as padding frames will be included.
    pub(crate) gapless: bool,

    /// An extension hint for the decoder about the format of the stream.
    /// When known, this can help the decoder to select the correct codec.
    pub(crate) hint: Option<String>,

    /// An MIME type hint for the decoder about the format of the stream.
    /// When known, this can help the decoder to select the correct demuxer.
    pub(crate) mime_type: Option<String>,

    /// Whether the decoder should report as seekable.
    pub(crate) is_seekable: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            byte_len: None,
            coarse_seek: false,
            gapless: true,
            hint: None,
            mime_type: None,
            is_seekable: false,
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
/// use rodio::decoder::DecoderBuilder;
///
/// let file = File::open("audio.mp3").unwrap();
/// let decoder = DecoderBuilder::new()
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
}

impl<R> Default for DecoderBuilder<R> {
    fn default() -> Self {
        Self {
            data: None,
            settings: Settings::default(),
        }
    }
}

impl<R: Read + Seek + Send + Sync + 'static> DecoderBuilder<R> {
    /// Creates a new decoder builder with default settings.
    ///
    /// # Examples
    /// ```no_run
    /// use std::fs::File;
    /// use rodio::decoder::DecoderBuilder;
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let decoder = DecoderBuilder::new()
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
    /// This is required for:
    /// - Reliable seeking operations
    /// - Duration calculations in formats that lack timing information (e.g. MP3, Vorbis)
    ///
    /// The byte length should typically be obtained from file metadata:
    /// ```no_run
    /// use std::fs::File;
    /// use rodio::Decoder;
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let len = file.metadata().unwrap().len();
    /// let decoder = Decoder::builder()
    ///     .with_data(file)
    ///     .with_byte_len(len)
    ///     .build()
    ///     .unwrap();
    /// ```
    ///
    /// Alternatively, it can be obtained by seeking to the end of the stream.
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
    /// When known, this can help the decoder to select the correct codec.
    /// Common values are "mp3", "wav", "flac", "ogg", etc.
    pub fn with_hint(mut self, hint: &str) -> Self {
        self.settings.hint = Some(hint.to_string());
        self
    }

    /// Sets a mime type hint for the decoder.
    ///
    /// When known, this can help the decoder to select the correct demuxer.
    /// Common values are "audio/mpeg", "audio/vnd.wav", "audio/flac", "audio/ogg", etc.
    pub fn with_mime_type(mut self, mime_type: &str) -> Self {
        self.settings.mime_type = Some(mime_type.to_string());
        self
    }

    /// Configure whether the decoder should report as seekable.
    ///
    /// For reliable seeking behavior, `byte_len` should be set either from file metadata
    /// or by seeking to the end of the stream. While seeking may work without byte_len
    /// for some formats, it is not guaranteed.
    ///
    /// # Examples
    /// ```no_run
    /// use std::fs::File;
    /// use rodio::Decoder;
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let len = file.metadata().unwrap().len();
    ///
    /// // Recommended: Set both byte_len and seekable
    /// let decoder = Decoder::builder()
    ///     .with_data(file)
    ///     .with_byte_len(len)
    ///     .with_seekable(true)
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn with_seekable(mut self, is_seekable: bool) -> Self {
        self.settings.is_seekable = is_seekable;
        self
    }

    /// Creates the decoder implementation with configured settings.
    fn build_impl(self) -> Result<(DecoderImpl<R>, Settings), DecoderError> {
        let data = self.data.ok_or(DecoderError::UnrecognizedFormat)?;

        #[cfg(all(feature = "wav", not(feature = "symphonia-wav")))]
        let data = match wav::WavDecoder::new(data) {
            Ok(decoder) => return Ok((DecoderImpl::Wav(decoder), self.settings)),
            Err(data) => data,
        };
        #[cfg(all(feature = "flac", not(feature = "symphonia-flac")))]
        let data = match flac::FlacDecoder::new(data) {
            Ok(decoder) => return Ok((DecoderImpl::Flac(decoder), self.settings)),
            Err(data) => data,
        };

        #[cfg(all(feature = "vorbis", not(feature = "symphonia-vorbis")))]
        let data = match vorbis::VorbisDecoder::new(data) {
            Ok(decoder) => return Ok((DecoderImpl::Vorbis(decoder), self.settings)),
            Err(data) => data,
        };

        #[cfg(all(feature = "minimp3", not(feature = "symphonia-mp3")))]
        let data = match mp3::Mp3Decoder::new(data) {
            Ok(decoder) => return Ok((DecoderImpl::Mp3(decoder), self.settings)),
            Err(data) => data,
        };

        #[cfg(feature = "symphonia")]
        {
            let mss = MediaSourceStream::new(
                Box::new(ReadSeekSource::new(data, &self.settings)) as Box<dyn MediaSource>,
                Default::default(),
            );

            symphonia::SymphoniaDecoder::new(mss, &self.settings)
                .map(|decoder| (DecoderImpl::Symphonia(decoder, PhantomData), self.settings))
        }

        #[cfg(not(feature = "symphonia"))]
        Err(DecoderError::UnrecognizedFormat)
    }

    /// Creates a new decoder with previously configured settings.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined
    /// or is not supported.
    pub fn build(self) -> Result<Decoder<R>, DecoderError> {
        let (decoder, _) = self.build_impl()?;
        Ok(Decoder(decoder))
    }

    /// Creates a new looped decoder with previously configured settings.
    ///
    /// # Errors
    ///
    /// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined
    /// or is not supported.
    pub fn build_looped(self) -> Result<LoopedDecoder<R>, DecoderError> {
        let (decoder, settings) = self.build_impl()?;
        Ok(LoopedDecoder {
            inner: Some(decoder),
            settings,
        })
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
            DecoderImpl::Symphonia(source, PhantomData) => source.next(),
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
            DecoderImpl::Symphonia(source, PhantomData) => source.size_hint(),
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
            DecoderImpl::Symphonia(source, PhantomData) => source.current_span_len(),
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
            DecoderImpl::Symphonia(source, PhantomData) => source.channels(),
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
            DecoderImpl::Symphonia(source, PhantomData) => source.sample_rate(),
        }
    }

    /// Returns the total duration of this audio source.
    ///
    /// # Symphonia Notes
    ///
    /// For formats that lack timing information like MP3 and Vorbis, this requires the decoder to
    /// be initialized with the correct byte length via `Decoder::builder().with_byte_len()`.
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
            DecoderImpl::Symphonia(source, PhantomData) => source.total_duration(),
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
            DecoderImpl::Symphonia(source, PhantomData) => source.try_seek(pos),
        }
    }
}

/// Converts a `File` into a `Decoder` with automatic optimizations.
/// This is the preferred way to decode files as it enables seeking optimizations
/// and accurate duration calculations.
///
/// This implementation:
/// - Wraps the file in a `BufReader` for better performance
/// - Gets the file length from metadata to improve seeking operations and duration accuracy
/// - Enables seeking by default
///
/// # Errors
///
/// Returns an error if:
/// - The file metadata cannot be read
/// - The audio format cannot be recognized or is not supported
///
/// # Examples
/// ```no_run
/// use std::fs::File;
/// use rodio::Decoder;
///
/// let file = File::open("audio.mp3").unwrap();
/// let decoder = Decoder::try_from(file).unwrap();
/// ```
impl TryFrom<std::fs::File> for Decoder<BufReader<std::fs::File>> {
    type Error = DecoderError;

    fn try_from(file: std::fs::File) -> Result<Self, Self::Error> {
        let len = file
            .metadata()
            .map_err(|e| Self::Error::IoError(e.to_string()))?
            .len();

        Self::builder()
            .with_data(BufReader::new(file))
            .with_byte_len(len)
            .with_seekable(true)
            .build()
    }
}

/// Converts a `BufReader` into a `Decoder`.
/// When working with files, prefer `TryFrom<File>` as it will automatically set byte_len
/// for better seeking performance.
///
/// # Errors
///
/// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined
/// or is not supported.
///
/// # Examples
/// ```no_run
/// use std::fs::File;
/// use std::io::BufReader;
/// use rodio::Decoder;
///
/// let file = File::open("audio.mp3").unwrap();
/// let reader = BufReader::new(file);
/// let decoder = Decoder::try_from(reader).unwrap();
/// ```
impl<R> TryFrom<BufReader<R>> for Decoder<BufReader<R>>
where
    R: Read + Seek + Send + Sync + 'static,
{
    type Error = DecoderError;

    fn try_from(data: BufReader<R>) -> Result<Self, Self::Error> {
        Self::new(data)
    }
}

/// Converts a `Cursor` into a `Decoder`.
/// When working with files, prefer `TryFrom<File>` as it will automatically set byte_len
/// for better seeking performance.
///
/// This is useful for decoding audio data that's already in memory.
///
/// # Errors
///
/// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined
/// or is not supported.
///
/// # Examples
/// ```no_run
/// use std::io::Cursor;
/// use rodio::Decoder;
///
/// let data = std::fs::read("audio.mp3").unwrap();
/// let cursor = Cursor::new(data);
/// let decoder = Decoder::try_from(cursor).unwrap();
/// ```
impl<T> TryFrom<std::io::Cursor<T>> for Decoder<std::io::Cursor<T>>
where
    T: AsRef<[u8]> + Send + Sync + 'static,
{
    type Error = DecoderError;

    fn try_from(data: std::io::Cursor<T>) -> Result<Self, Self::Error> {
        Self::new(data)
    }
}

impl<R: Read + Seek + Send + Sync + 'static> Decoder<R> {
    /// Returns a builder for creating a new decoder with customizable settings.
    ///
    /// # Examples
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
        DecoderBuilder::new().with_data(data).build()
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
        DecoderBuilder::new().with_data(data).build_looped()
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
        DecoderBuilder::new()
            .with_data(data)
            .with_hint("wav")
            .build()
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
        DecoderBuilder::new()
            .with_data(data)
            .with_hint("flac")
            .build()
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
        DecoderBuilder::new()
            .with_data(data)
            .with_hint("ogg")
            .build()
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
        DecoderBuilder::new()
            .with_data(data)
            .with_hint("mp3")
            .build()
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
        DecoderBuilder::new()
            .with_data(data)
            .with_hint("aac")
            .build()
    }

    /// Builds a new decoder with MP4 container format hint.
    ///
    /// This method provides a hint that the data is in MP4 container format by setting
    /// the MIME type to "audio/mp4". This may help the decoder identify the format
    /// more quickly. However, if MP4 decoding fails, other formats will still be attempted.
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
    /// let file = File::open("audio.m4a").unwrap();
    /// let decoder = Decoder::new_mp4(file).unwrap();
    /// ```
    #[cfg(feature = "symphonia-isomp4")]
    pub fn new_mp4(data: R) -> Result<Self, DecoderError> {
        DecoderBuilder::new()
            .with_data(data)
            .with_mime_type("audio/mp4")
            .build()
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

    /// Returns the next sample in the audio stream.
    ///
    /// When the end of the stream is reached, attempts to seek back to the start
    /// and continue playing. If seeking fails, or if no decoder is available,
    /// returns `None`.
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(inner) = &mut self.inner {
            if let Some(sample) = inner.next() {
                return Some(sample);
            }

            // Take ownership of the decoder to reset it
            let decoder = self.inner.take()?;
            let (new_decoder, sample) = match decoder {
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
                DecoderImpl::Symphonia(source, PhantomData) => {
                    let mut reader = source.into_inner();
                    reader.seek(SeekFrom::Start(0)).ok()?;
                    let mut source =
                        symphonia::SymphoniaDecoder::new(reader, &self.settings).ok()?;
                    let sample = source.next();
                    (DecoderImpl::Symphonia(source, PhantomData), sample)
                }
            };
            self.inner = Some(new_decoder);
            sample
        } else {
            None
        }
    }

    /// Returns the size hint for this iterator.
    ///
    /// The lower bound is:
    /// - The minimum number of samples remaining in the current iteration if there is an active decoder
    /// - 0 if there is no active decoder (inner is None)
    ///
    /// The upper bound is always `None` since the decoder loops indefinitely.
    /// This differs from non-looped decoders which may provide a finite upper bound.
    ///
    /// Note that even with an active decoder, reaching the end of the stream may result
    /// in the decoder becoming inactive if seeking back to the start fails.
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (
            self.inner.as_ref().map_or(0, |inner| inner.size_hint().0),
            None,
        )
    }
}

impl<R> Source for LoopedDecoder<R>
where
    R: Read + Seek,
{
    /// Returns the current span length of the underlying decoder.
    ///
    /// Returns `None` if there is no active decoder.
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.inner.as_ref()?.current_span_len()
    }

    /// Returns the number of channels in the audio stream.
    ///
    /// Returns the default channel count if there is no active decoder.
    #[inline]
    fn channels(&self) -> ChannelCount {
        self.inner
            .as_ref()
            .map_or(ChannelCount::default(), |inner| inner.channels())
    }

    /// Returns the sample rate of the audio stream.
    ///
    /// Returns the default sample rate if there is no active decoder.
    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.inner
            .as_ref()
            .map_or(SampleRate::default(), |inner| inner.sample_rate())
    }

    /// Returns the total duration of this audio source.
    ///
    /// Always returns `None` for looped decoders since they have no fixed end point -
    /// they will continue playing indefinitely by seeking back to the start when reaching
    /// the end of the audio data.
    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    /// Attempts to seek to a specific position in the audio stream.
    ///
    /// # Errors
    ///
    /// Returns `SeekError::NotSupported` if:
    /// - There is no active decoder
    /// - The underlying decoder does not support seeking
    ///
    /// May also return other `SeekError` variants if the underlying decoder's seek operation fails.
    ///
    /// # Note
    ///
    /// Even for looped playback, seeking past the end of the stream will not automatically
    /// wrap around to the beginning - it will return an error just like a normal decoder.
    /// Looping only occurs when reaching the end through normal playback.
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        match &mut self.inner {
            Some(inner) => inner.try_seek(pos),
            None => Err(SeekError::Other(Box::new(DecoderError::IoError(
                "Looped source ended when it failed to loop back".to_string(),
            )))),
        }
    }
}

/// Errors that can occur when creating a decoder.
#[derive(Debug, Clone)]
pub enum DecoderError {
    /// The format of the data has not been recognized.
    UnrecognizedFormat,

    /// An IO error occurred while reading, writing, or seeking the stream.
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

    /// No streams were found by the decoder.
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
