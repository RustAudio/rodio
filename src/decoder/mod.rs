//! Decodes audio samples from various audio file formats.
//!
//! This module provides decoders for common audio formats like MP3, WAV, Vorbis and FLAC.
//! It supports both one-shot playback and looped playback of audio files.
//!
//! # Usage
//!
//! The simplest way to decode files (automatically sets up seeking and duration):
//! ```no_run
//! use std::fs::File;
//! use rodio::Decoder;
//!
//! let file = File::open("audio.mp3").unwrap();
//! let decoder = Decoder::try_from(file).unwrap();  // Automatically sets byte_len from metadata
//! ```
//!
//! For more control over decoder settings, use the builder pattern:
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
//!     .with_gapless(true)      // Enable gapless playback
//!     .build()
//!     .unwrap();
//! ```
//!
//! # Features
//!
//! The following audio formats are supported based on enabled features:
//!
//! - `wav` - WAV format support
//! - `flac` - FLAC format support
//! - `vorbis` - Vorbis format support
//! - `mp3` - MP3 format support via minimp3
//! - `symphonia` - Enhanced format support via the Symphonia backend
//!
//! When using `symphonia`, additional formats like AAC and MP4 containers become available
//! if the corresponding features are enabled.

use std::{
    io::{BufReader, Read, Seek},
    marker::PhantomData,
    sync::Arc,
    time::Duration,
};

#[allow(unused_imports)]
use std::io::SeekFrom;

use crate::{
    common::{assert_error_traits, ChannelCount, SampleRate},
    math::nz,
    source::{SeekError, Source},
    Sample,
};

pub mod builder;
pub use builder::DecoderBuilder;
use builder::Settings;

mod utils;

#[cfg(feature = "claxon")]
mod flac;
#[cfg(feature = "minimp3")]
mod mp3;
#[cfg(feature = "symphonia")]
mod read_seek_source;
#[cfg(feature = "symphonia")]
mod symphonia;
#[cfg(feature = "lewton")]
mod vorbis;
#[cfg(feature = "hound")]
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
/// For seekable sources with gapless playback enabled, this uses `try_seek(Duration::ZERO)`
/// which is fast. For non-seekable sources or when gapless is disabled, it recreates the
/// decoder but caches metadata from the first iteration to avoid expensive file scanning
/// on subsequent loops.
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
#[allow(dead_code)]
pub struct LoopedDecoder<R: Read + Seek> {
    /// The underlying decoder implementation.
    inner: Option<DecoderImpl<R>>,
    /// Configuration settings for the decoder.
    settings: Settings,
    /// Cached metadata from the first successful decoder creation.
    /// Used to avoid expensive file scanning on subsequent loops.
    cached_duration: Option<Duration>,
}

// Cannot really reduce the size of the VorbisDecoder. There are not any
/// Internal enum representing different decoder implementations.
///
/// This enum dispatches to the appropriate decoder based on detected format
/// and available features. Large enum variant size is acceptable here since
/// these are infrequently created and moved.
#[allow(clippy::large_enum_variant)]
enum DecoderImpl<R: Read + Seek> {
    /// WAV decoder using hound library
    #[cfg(feature = "hound")]
    Wav(wav::WavDecoder<R>),
    /// Ogg Vorbis decoder using lewton library
    #[cfg(feature = "lewton")]
    Vorbis(vorbis::VorbisDecoder<R>),
    /// FLAC decoder using claxon library
    #[cfg(feature = "claxon")]
    Flac(flac::FlacDecoder<R>),
    /// MP3 decoder using minimp3 library
    #[cfg(feature = "minimp3")]
    Mp3(mp3::Mp3Decoder<R>),
    /// Multi-format decoder using symphonia library
    #[cfg(feature = "symphonia")]
    Symphonia(symphonia::SymphoniaDecoder, PhantomData<R>),
    /// Placeholder variant to satisfy compiler when no decoders are enabled.
    /// This variant is unreachable and should never be constructed.
    #[allow(dead_code)]
    None(Unreachable, PhantomData<R>),
}

/// Placeholder type for the None variant that can never be instantiated.
enum Unreachable {}

impl<R: Read + Seek> DecoderImpl<R> {
    /// Advances the decoder and returns the next sample.
    #[inline]
    fn next(&mut self) -> Option<Sample> {
        match self {
            #[cfg(feature = "hound")]
            DecoderImpl::Wav(source) => source.next(),
            #[cfg(feature = "lewton")]
            DecoderImpl::Vorbis(source) => source.next(),
            #[cfg(feature = "claxon")]
            DecoderImpl::Flac(source) => source.next(),
            #[cfg(feature = "minimp3")]
            DecoderImpl::Mp3(source) => source.next(),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source, PhantomData) => source.next(),
            DecoderImpl::None(_, _) => unreachable!(),
        }
    }

    /// Returns the bounds on the remaining amount of samples.
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            #[cfg(feature = "hound")]
            DecoderImpl::Wav(source) => source.size_hint(),
            #[cfg(feature = "lewton")]
            DecoderImpl::Vorbis(source) => source.size_hint(),
            #[cfg(feature = "claxon")]
            DecoderImpl::Flac(source) => source.size_hint(),
            #[cfg(feature = "minimp3")]
            DecoderImpl::Mp3(source) => source.size_hint(),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source, PhantomData) => source.size_hint(),
            DecoderImpl::None(_, _) => unreachable!(),
        }
    }

    /// Returns the number of samples before the current span ends.
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        match self {
            #[cfg(feature = "hound")]
            DecoderImpl::Wav(source) => source.current_span_len(),
            #[cfg(feature = "lewton")]
            DecoderImpl::Vorbis(source) => source.current_span_len(),
            #[cfg(feature = "claxon")]
            DecoderImpl::Flac(source) => source.current_span_len(),
            #[cfg(feature = "minimp3")]
            DecoderImpl::Mp3(source) => source.current_span_len(),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source, PhantomData) => source.current_span_len(),
            DecoderImpl::None(_, _) => unreachable!(),
        }
    }

    /// Returns the number of audio channels.
    #[inline]
    fn channels(&self) -> ChannelCount {
        match self {
            #[cfg(feature = "hound")]
            DecoderImpl::Wav(source) => source.channels(),
            #[cfg(feature = "lewton")]
            DecoderImpl::Vorbis(source) => source.channels(),
            #[cfg(feature = "claxon")]
            DecoderImpl::Flac(source) => source.channels(),
            #[cfg(feature = "minimp3")]
            DecoderImpl::Mp3(source) => source.channels(),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source, PhantomData) => source.channels(),
            DecoderImpl::None(_, _) => unreachable!(),
        }
    }

    /// Returns the sample rate in Hz.
    #[inline]
    fn sample_rate(&self) -> SampleRate {
        match self {
            #[cfg(feature = "hound")]
            DecoderImpl::Wav(source) => source.sample_rate(),
            #[cfg(feature = "lewton")]
            DecoderImpl::Vorbis(source) => source.sample_rate(),
            #[cfg(feature = "claxon")]
            DecoderImpl::Flac(source) => source.sample_rate(),
            #[cfg(feature = "minimp3")]
            DecoderImpl::Mp3(source) => source.sample_rate(),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source, PhantomData) => source.sample_rate(),
            DecoderImpl::None(_, _) => unreachable!(),
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
            #[cfg(feature = "hound")]
            DecoderImpl::Wav(source) => source.total_duration(),
            #[cfg(feature = "lewton")]
            DecoderImpl::Vorbis(source) => source.total_duration(),
            #[cfg(feature = "claxon")]
            DecoderImpl::Flac(source) => source.total_duration(),
            #[cfg(feature = "minimp3")]
            DecoderImpl::Mp3(source) => source.total_duration(),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source, PhantomData) => source.total_duration(),
            DecoderImpl::None(_, _) => unreachable!(),
        }
    }

    /// Returns the bits per sample of this audio source.
    ///
    /// # Format Support
    ///
    /// For lossy formats this should always return `None` as bit depth is not a meaningful
    /// concept for compressed audio.
    #[inline]
    fn bits_per_sample(&self) -> Option<u32> {
        match self {
            #[cfg(feature = "hound")]
            DecoderImpl::Wav(source) => source.bits_per_sample(),
            #[cfg(feature = "lewton")]
            DecoderImpl::Vorbis(source) => source.bits_per_sample(),
            #[cfg(feature = "claxon")]
            DecoderImpl::Flac(source) => source.bits_per_sample(),
            #[cfg(feature = "minimp3")]
            DecoderImpl::Mp3(source) => source.bits_per_sample(),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source, PhantomData) => source.bits_per_sample(),
            DecoderImpl::None(_, _) => unreachable!(),
        }
    }

    /// Attempts to seek to a given position in the current source.
    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        match self {
            #[cfg(feature = "hound")]
            DecoderImpl::Wav(source) => source.try_seek(pos),
            #[cfg(feature = "lewton")]
            DecoderImpl::Vorbis(source) => source.try_seek(pos),
            #[cfg(feature = "claxon")]
            DecoderImpl::Flac(source) => source.try_seek(pos),
            #[cfg(feature = "minimp3")]
            DecoderImpl::Mp3(source) => source.try_seek(pos),
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source, PhantomData) => source.try_seek(pos),
            DecoderImpl::None(_, _) => unreachable!(),
        }
    }
}

/// Converts a `File` into a `Decoder`.
///
/// This is the recommended way to decode audio files from the filesystem. The file is
/// automatically wrapped in a `BufReader` for efficient I/O, and the decoder will know the exact
/// file size for optimal seeking performance.
///
/// # Errors
///
/// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined or is
/// not supported.
///
/// Returns `DecoderError::IoError` if the file metadata cannot be read.
///
/// # Examples
/// ```no_run
/// use std::fs::File;
/// use rodio::Decoder;
///
/// let file = File::open("music.mp3").unwrap();
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

/// Converts a `BufReader<R>` into a `Decoder`.
///
/// This is useful for decoding from any readable and seekable source wrapped in a `BufReader`.
/// When working with files specifically, prefer `TryFrom<File>` as it automatically determines the
/// file size for better seeking performance.
///
/// # Errors
///
/// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined or is
/// not supported.
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
        Self::builder().with_data(data).with_seekable(true).build()
    }
}

/// Converts a `Cursor<T>` into a `Decoder`.
///
/// This is useful for decoding audio data that's already wrapped in a `Cursor`. The decoder will
/// know the exact size of the data for efficient seeking and duration calculation.
///
/// For unwrapped byte containers, prefer the direct `TryFrom` implementations for `Vec<u8>`,
/// `Box<[u8]>`, `Arc<[u8]>`, or `bytes::Bytes`.
///
/// # Errors
///
/// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined or is
/// not supported.
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
        let len = data.get_ref().as_ref().len() as u64;

        Self::builder()
            .with_data(data)
            .with_byte_len(len)
            .with_seekable(true)
            .build()
    }
}

/// Helper function to create a decoder from data that can be converted to bytes.
///
/// This function wraps the data in a `Cursor` and configures the decoder with optimal settings for
/// in-memory audio data: known byte length and seeking enabled for better performance.
fn decoder_from_bytes<T>(data: T) -> Result<Decoder<std::io::Cursor<T>>, DecoderError>
where
    T: AsRef<[u8]> + Send + Sync + 'static,
{
    let len = data.as_ref().len() as u64;
    let cursor = std::io::Cursor::new(data);

    Decoder::builder()
        .with_data(cursor)
        .with_byte_len(len)
        .with_seekable(true)
        .build()
}

/// Converts a `Vec<u8>` into a `Decoder`.
///
/// This is useful for decoding audio data that's loaded into memory as a vector. The data is
/// wrapped in a `Cursor` to provide seeking capabilities. The decoder will know the exact size of
/// the audio data, enabling efficient seeking.
///
/// # Errors
///
/// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined or is
/// not supported.
///
/// # Examples
/// ```no_run
/// use rodio::Decoder;
///
/// // Load audio file into memory
/// let audio_data = std::fs::read("music.mp3").unwrap();
/// let decoder = Decoder::try_from(audio_data).unwrap();
/// ```
impl TryFrom<Vec<u8>> for Decoder<std::io::Cursor<Vec<u8>>> {
    type Error = DecoderError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        decoder_from_bytes(data)
    }
}

/// Converts a `Box<[u8]>` into a `Decoder`.
///
/// This is useful for decoding audio data with exact memory allocation (no extra capacity like
/// `Vec<u8>` might have). The boxed slice is memory-efficient and signals that the audio data is
/// immutable and final.
///
/// # Errors
///
/// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined or is
/// not supported.
///
/// # Examples
/// ```no_run
/// use rodio::Decoder;
///
/// let audio_vec = std::fs::read("audio.flac").unwrap();
/// let audio_box: Box<[u8]> = audio_vec.into_boxed_slice();
/// let decoder = Decoder::try_from(audio_box).unwrap();
/// ```
impl TryFrom<Box<[u8]>> for Decoder<std::io::Cursor<Box<[u8]>>> {
    type Error = DecoderError;

    fn try_from(data: Box<[u8]>) -> Result<Self, Self::Error> {
        decoder_from_bytes(data)
    }
}

/// Converts an `Arc<[u8]>` into a `Decoder`.
///
/// This is useful for sharing audio data across multiple decoders or threads without copying the
/// underlying bytes. Perfect for scenarios where you need multiple decoders for the same audio
/// data (e.g., playing overlapping sound effects in games).
///
/// # Errors
///
/// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined or is
/// not supported.
///
/// # Examples
/// ```no_run
/// use std::sync::Arc;
/// use rodio::Decoder;
///
/// let audio_data: Arc<[u8]> = Arc::from(std::fs::read("sound.wav").unwrap());
///
/// // Create multiple decoders sharing the same data (no copying!)
/// let decoder1 = Decoder::try_from(audio_data.clone()).unwrap();
/// let decoder2 = Decoder::try_from(audio_data).unwrap();
/// ```
impl TryFrom<std::sync::Arc<[u8]>> for Decoder<std::io::Cursor<std::sync::Arc<[u8]>>> {
    type Error = DecoderError;

    fn try_from(data: std::sync::Arc<[u8]>) -> Result<Self, Self::Error> {
        decoder_from_bytes(data)
    }
}

/// Converts a `bytes::Bytes` into a `Decoder`.
///
/// This is particularly useful in async/web applications where audio data is received from HTTP
/// clients, message queues, or other network sources. `Bytes` provides efficient, reference-counted
/// sharing of byte data.
///
/// This implementation is only available when the `bytes` feature is enabled.
///
/// # Errors
///
/// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined or is
/// not supported.
///
/// # Examples
/// ```ignore
/// use rodio::Decoder;
/// use bytes::Bytes;
///
/// // Common in web applications
/// let audio_response = reqwest::get("https://example.com/audio.mp3").await.unwrap();
/// let audio_bytes: Bytes = audio_response.bytes().await.unwrap();
/// let decoder = Decoder::try_from(audio_bytes).unwrap();
/// ```
#[cfg(feature = "bytes")]
#[cfg_attr(docsrs, doc(cfg(feature = "bytes")))]
impl TryFrom<bytes::Bytes> for Decoder<std::io::Cursor<bytes::Bytes>> {
    type Error = DecoderError;

    fn try_from(data: bytes::Bytes) -> Result<Self, Self::Error> {
        decoder_from_bytes(data)
    }
}

/// Converts a `&'static [u8]` into a `Decoder`.
///
/// This is useful for decoding audio data that's embedded directly in the binary, such as sound
/// effects in games or applications. The static lifetime ensures the data remains valid for the
/// decoder's lifetime.
///
/// # Errors
///
/// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined or is
/// not supported.
///
/// # Examples
/// ```no_run
/// use rodio::Decoder;
///
/// // Embedded audio data (e.g., from include_bytes!)
/// static AUDIO_DATA: &[u8] = include_bytes!("../../assets/music.wav");
/// let decoder = Decoder::try_from(AUDIO_DATA).unwrap();
/// ```
impl TryFrom<&'static [u8]> for Decoder<std::io::Cursor<&'static [u8]>> {
    type Error = DecoderError;

    fn try_from(data: &'static [u8]) -> Result<Self, Self::Error> {
        decoder_from_bytes(data)
    }
}

/// Converts a `Cow<'static, [u8]>` into a `Decoder`.
///
/// This is useful for APIs that want to accept either borrowed static data or owned data without
/// requiring callers to choose upfront. The cow can contain either embedded audio data or
/// dynamically loaded data.
///
/// # Errors
///
/// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined or is
/// not supported.
///
/// # Examples
/// ```no_run
/// use rodio::Decoder;
/// use rodio::decoder::DecoderError;
/// use std::borrow::Cow;
///
/// // Can accept both owned and borrowed data
/// fn decode_audio(data: Cow<'static, [u8]>) -> Result<Decoder<std::io::Cursor<std::borrow::Cow<'static, [u8]>>>, DecoderError> {
///     Decoder::try_from(data)
/// }
///
/// static EMBEDDED: &[u8] = include_bytes!("../../assets/music.wav");
/// let decoder1 = decode_audio(Cow::Borrowed(EMBEDDED)).unwrap();
/// let owned_data = std::fs::read("music.wav").unwrap();
/// let decoder2 = decode_audio(Cow::Owned(owned_data)).unwrap();
/// ```
impl TryFrom<std::borrow::Cow<'static, [u8]>>
    for Decoder<std::io::Cursor<std::borrow::Cow<'static, [u8]>>>
{
    type Error = DecoderError;

    fn try_from(data: std::borrow::Cow<'static, [u8]>) -> Result<Self, Self::Error> {
        decoder_from_bytes(data)
    }
}

/// Converts a `&Path` into a `Decoder`.
///
/// This is a convenience method for loading audio files from filesystem paths. The file is opened
/// and automatically configured with optimal settings including file size detection, seeking
/// support and format hint.
///
/// # Errors
///
/// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined or is
/// not supported.
///
/// Returns `DecoderError::IoError` if the file cannot be opened or its metadata cannot be read.
///
/// # Examples
/// ```no_run
/// use rodio::Decoder;
/// use std::path::Path;
///
/// let path = Path::new("music.mp3");
/// let decoder = Decoder::try_from(path).unwrap();
/// ```
impl TryFrom<&std::path::Path> for Decoder<BufReader<std::fs::File>> {
    type Error = DecoderError;

    fn try_from(path: &std::path::Path) -> Result<Self, Self::Error> {
        path.to_path_buf().try_into()
    }
}

/// Converts a `PathBuf` into a `Decoder`.
///
/// This is a convenience method for loading audio files from filesystem paths. The file is opened
/// and automatically configured with optimal settings including file size detection, seeking
/// support and format hint.
///
/// # Errors
///
/// Returns `DecoderError::UnrecognizedFormat` if the audio format could not be determined or is
/// not supported.
///
/// Returns `DecoderError::IoError` if the file cannot be opened or its metadata cannot be read.
///
/// # Examples
/// ```no_run
/// use rodio::Decoder;
/// use std::path::PathBuf;
///
/// let path = PathBuf::from("music.mp3");
/// let decoder = Decoder::try_from(path).unwrap();
/// ```
impl TryFrom<std::path::PathBuf> for Decoder<BufReader<std::fs::File>> {
    type Error = DecoderError;

    fn try_from(path: std::path::PathBuf) -> Result<Self, Self::Error> {
        let ext = path.extension().and_then(|e| e.to_str());
        let file = std::fs::File::open(&path).map_err(|e| DecoderError::IoError(e.to_string()))?;

        let len = file
            .metadata()
            .map_err(|e| DecoderError::IoError(e.to_string()))?
            .len();

        let mut builder = Self::builder()
            .with_data(BufReader::new(file))
            .with_byte_len(len)
            .with_seekable(true);

        if let Some(ext) = ext {
            let hint = match ext {
                "adif" | "adts" => "aac",
                "caf" => "audio/x-caf",
                "m4a" | "m4b" | "m4p" | "m4r" | "mp4" => "audio/mp4",
                "bit" | "mpga" => "mp3",
                "mka" | "mkv" => "audio/matroska",
                "oga" | "ogm" | "ogv" | "ogx" | "spx" => "audio/ogg",
                "wave" => "wav",
                _ => ext,
            };
            builder = builder.with_hint(hint);
        }

        builder.build()
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
    /// Attempts to automatically detect the format of the source of data, but does not determine
    /// byte length or enable seeking by default. If you are working with a `File`, then you will
    /// probably want to use `Decoder::try_from(file)` instead.
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
    /// Attempts to automatically detect the format of the source of data, but does not determine
    /// byte length or enable seeking by default. If you are working with a `File`, then you will
    /// probably want to use `Decoder::try_from(file)` instead.
    ///
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
    #[cfg(any(
        feature = "hound",
        all(feature = "symphonia-pcm", feature = "symphonia-wav")
    ))]
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
    #[cfg(any(feature = "claxon", feature = "symphonia-flac"))]
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
    #[cfg(any(
        feature = "lewton",
        all(feature = "symphonia-ogg", feature = "symphonia-vorbis")
    ))]
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
    #[cfg(all(feature = "symphonia-aac", feature = "symphonia-isomp4"))]
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
    #[cfg(all(feature = "symphonia-aac", feature = "symphonia-isomp4"))]
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
    fn bits_per_sample(&self) -> Option<u32> {
        self.0.bits_per_sample()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.0.try_seek(pos)
    }
}

impl<R> LoopedDecoder<R>
where
    R: Read + Seek,
{
    /// Recreate decoder with cached metadata to avoid expensive file scanning.
    fn recreate_decoder_with_cache(
        &mut self,
        decoder: DecoderImpl<R>,
    ) -> Option<(DecoderImpl<R>, Option<Sample>)> {
        // Create settings with cached metadata for fast recreation.
        // Note: total_duration is important even though LoopedDecoder::total_duration()  returns
        // None, because the individual decoder's total_duration() is used for seek saturation
        // (clamping seeks beyond the end to the end position).
        let mut fast_settings = self.settings.clone();
        fast_settings.total_duration = self.cached_duration;

        let (new_decoder, sample) = match decoder {
            #[cfg(feature = "hound")]
            DecoderImpl::Wav(source) => {
                let mut reader = source.into_inner();
                reader.rewind().ok()?;
                let mut source = wav::WavDecoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                (DecoderImpl::Wav(source), sample)
            }
            #[cfg(feature = "lewton")]
            DecoderImpl::Vorbis(source) => {
                let mut reader = source.into_inner().into_inner().into_inner();
                reader.rewind().ok()?;
                let mut source =
                    vorbis::VorbisDecoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                (DecoderImpl::Vorbis(source), sample)
            }
            #[cfg(feature = "claxon")]
            DecoderImpl::Flac(source) => {
                let mut reader = source.into_inner();
                reader.rewind().ok()?;
                let mut source =
                    flac::FlacDecoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                (DecoderImpl::Flac(source), sample)
            }
            #[cfg(feature = "minimp3")]
            DecoderImpl::Mp3(source) => {
                let mut reader = source.into_inner();
                reader.rewind().ok()?;
                let mut source = mp3::Mp3Decoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                (DecoderImpl::Mp3(source), sample)
            }
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source, PhantomData) => {
                let mut reader = source.into_inner();
                reader.rewind().ok()?;
                let mut source =
                    symphonia::SymphoniaDecoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                (DecoderImpl::Symphonia(source, PhantomData), sample)
            }
            DecoderImpl::None(_, _) => return None,
        };
        Some((new_decoder, sample))
    }
}

impl<R> Iterator for LoopedDecoder<R>
where
    R: Read + Seek,
{
    type Item = Sample;

    /// Returns the next sample in the audio stream.
    ///
    /// When the end of the stream is reached, attempts to seek back to the start and continue
    /// playing. For seekable sources with gapless playback, this uses fast seeking. For
    /// non-seekable sources or when gapless is disabled, recreates the decoder using cached
    /// metadata to avoid expensive file scanning.
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(inner) = &mut self.inner {
            if let Some(sample) = inner.next() {
                return Some(sample);
            }

            // Cache duration from current decoder before resetting (first time only)
            if self.cached_duration.is_none() {
                self.cached_duration = inner.total_duration();
            }

            // Try seeking first for seekable sources - this is fast and gapless
            // Only use fast seeking when gapless=true, otherwise recreate normally
            if self.settings.gapless
                && self.settings.is_seekable
                && inner.try_seek(Duration::ZERO).is_ok()
            {
                return inner.next();
            }

            // Fall back to recreation with cached metadata to avoid expensive scanning
            let decoder = self.inner.take()?;
            let (new_decoder, sample) = self.recreate_decoder_with_cache(decoder)?;
            self.inner = Some(new_decoder);
            sample
        } else {
            None
        }
    }

    /// Returns the size hint for this iterator.
    ///
    /// The lower bound is:
    /// - The minimum number of samples remaining in the current iteration if there is an active
    ///   decoder
    /// - 0 if there is no active decoder (inner is None)
    ///
    /// The upper bound is always `None` since the decoder loops indefinitely.
    ///
    /// Note that even with an active decoder, reaching the end of the stream may result in the
    /// decoder becoming inactive if seeking back to the start fails.
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
        self.inner.as_ref().map_or(nz!(1), |inner| inner.channels())
    }

    /// Returns the sample rate of the audio stream.
    ///
    /// Returns the default sample rate if there is no active decoder.
    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.inner
            .as_ref()
            .map_or(nz!(44100), |inner| inner.sample_rate())
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

    /// Returns the bits per sample of the underlying decoder, if available.
    #[inline]
    fn bits_per_sample(&self) -> Option<u32> {
        self.inner.as_ref()?.bits_per_sample()
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
            None => Err(SeekError::Other(Arc::new(DecoderError::IoError(
                "Looped source ended when it failed to loop back".to_string(),
            )))),
        }
    }
}

/// Errors that can occur when creating a decoder.
#[derive(Debug, thiserror::Error, Clone)]
pub enum DecoderError {
    /// The format of the data has not been recognized.
    #[error("The format of the data has not been recognized.")]
    UnrecognizedFormat,

    /// An IO error occurred while reading, writing, or seeking the stream.
    #[error("An IO error occurred while reading, writing, or seeking the stream.")]
    IoError(String),

    /// The stream contained malformed data and could not be decoded or demuxed.
    #[error("The stream contained malformed data and could not be decoded or demuxed: {0}")]
    #[cfg(feature = "symphonia")]
    #[cfg_attr(docsrs, doc(cfg(feature = "symphonia")))]
    DecodeError(&'static str),

    /// A default or user-defined limit was reached while decoding or demuxing
    /// the stream. Limits are used to prevent denial-of-service attacks from
    /// malicious streams.
    #[error(
        "A default or user-defined limit was reached while decoding or demuxing the stream: {0}"
    )]
    #[cfg(feature = "symphonia")]
    #[cfg_attr(docsrs, doc(cfg(feature = "symphonia")))]
    LimitError(&'static str),

    /// The demuxer or decoder needs to be reset before continuing.
    #[error("The demuxer or decoder needs to be reset before continuing.")]
    #[cfg(feature = "symphonia")]
    #[cfg_attr(docsrs, doc(cfg(feature = "symphonia")))]
    ResetRequired,

    /// No streams were found by the decoder.
    #[error("No streams were found by the decoder.")]
    #[cfg(feature = "symphonia")]
    #[cfg_attr(docsrs, doc(cfg(feature = "symphonia")))]
    NoStreams,
}
assert_error_traits!(DecoderError);
