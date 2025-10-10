//! Symphonia multi-format audio decoder implementation.
//!
//! This module provides comprehensive audio decoding using the `symphonia` library, which
//! supports multiple audio formats and containers. Symphonia is designed for high-performance
//! audio decoding with support for complex features like multi-track files, metadata parsing,
//! and format-specific optimizations.
//!
//! # Supported Formats
//!
//! - **Containers**: MP4, OGG, Matroska (MKV), FLAC, WAV, AIFF, CAF
//! - **Codecs**: AAC, FLAC, MP3, Vorbis, Opus, PCM, ALAC, WavPack
//! - **Features**: Multi-track files, embedded metadata, gapless playback
//! - **Advanced**: Chained Ogg streams, complex MP4 structures, codec changes
//!
//! # Capabilities
//!
//! - **Multi-track**: Automatic track selection for audio content
//! - **Seeking**: Precise seeking with timebase-aware positioning
//! - **Duration**: Metadata-based duration with track-specific timing
//! - **Performance**: Optimized decoding with format-specific implementations
//! - **Metadata**: Rich metadata parsing and handling (not exposed in this decoder)
//! - **Error recovery**: Robust handling of corrupted or incomplete streams
//!
//! # Advantages
//!
//! - **Universal support**: Single decoder for many formats
//! - **High performance**: Format-specific optimizations
//! - **Robust parsing**: Handles complex and non-standard files
//! - **Advanced features**: Multi-track, gapless, precise seeking
//!
//! # Configuration
//!
//! The decoder supports extensive configuration through `DecoderBuilder`:
//! - `with_hint("mp4")` - Format hint for faster detection
//! - `with_mime_type("audio/mp4")` - MIME type hint for format identification
//! - `with_gapless(true)` - Enable gapless playback for supported formats
//! - `with_seekable(true)` - Enable seeking support (required for backward seeks)
//! - `with_byte_len(len)` - Required for reliable seeking in some formats
//! - `with_seek_mode(SeekMode::Fastest)` - Use coarse seeking for speed
//! - `with_seek_mode(SeekMode::Nearest)` - Use precise seeking for accuracy
//!
//! # Performance Considerations
//!
//! - Format detection overhead for unknown formats (hints help)
//! - Memory usage scales with track complexity and buffer sizes
//! - Seeking performance varies significantly by container format
//! - Multi-track files may require additional processing overhead
//! - Some formats require byte length for optimal seeking performance
//!
//! # Seeking Behavior
//!
//! Seeking behavior varies by format and configuration:
//! - **MP3**: Requires byte length for coarse seeking, otherwise uses nearest
//! - **OGG**: Requires seekable flag, may fail silently if not set
//! - **MP4**: No automatic fallback between seek modes
//! - **FLAC/WAV**: Generally reliable seeking with proper configuration
//!
//! # Example
//!
//! ```no_run
//! use std::fs::File;
//! use rodio::{Decoder, Source, decoder::builder::SeekMode};
//!
//! let file = File::open("audio.m4a").unwrap();
//! let decoder = Decoder::builder()
//!     .with_data(file)
//!     .with_hint("m4a")
//!     .with_seekable(true)
//!     .with_gapless(true)
//!     .with_seek_mode(SeekMode::Fastest)
//!     .build()
//!     .unwrap();
//!
//! // Symphonia can detect bit depth for some formats
//! if let Some(bits) = decoder.bits_per_sample() {
//!     println!("Bit depth: {} bits", bits);
//! }
//! ```

use std::{
    io::{Read, Seek, SeekFrom},
    sync::Arc,
    time::Duration,
};

use symphonia::{
    core::{
        audio::SampleBuffer,
        codecs::{
            CodecType, Decoder, DecoderOptions, CODEC_TYPE_ALAC, CODEC_TYPE_FLAC,
            CODEC_TYPE_MONKEYS_AUDIO, CODEC_TYPE_NULL, CODEC_TYPE_PCM_ALAW, CODEC_TYPE_PCM_F32BE,
            CODEC_TYPE_PCM_F32LE, CODEC_TYPE_PCM_F64BE, CODEC_TYPE_PCM_F64LE, CODEC_TYPE_PCM_MULAW,
            CODEC_TYPE_PCM_S16BE, CODEC_TYPE_PCM_S16LE, CODEC_TYPE_PCM_S24BE, CODEC_TYPE_PCM_S24LE,
            CODEC_TYPE_PCM_S32BE, CODEC_TYPE_PCM_S32LE, CODEC_TYPE_PCM_S8, CODEC_TYPE_PCM_U16BE,
            CODEC_TYPE_PCM_U16LE, CODEC_TYPE_PCM_U24BE, CODEC_TYPE_PCM_U24LE, CODEC_TYPE_PCM_U32BE,
            CODEC_TYPE_PCM_U32LE, CODEC_TYPE_PCM_U8, CODEC_TYPE_TTA, CODEC_TYPE_VORBIS,
            CODEC_TYPE_WAVPACK,
        },
        errors::Error,
        formats::{FormatOptions, FormatReader, SeekMode as SymphoniaSeekMode, SeekTo},
        io::{MediaSource, MediaSourceStream},
        meta::MetadataOptions,
        probe::Hint,
    },
    default::get_probe,
};

use super::DecoderError;
use crate::{
    common::{ChannelCount, Sample, SampleRate},
    decoder::builder::SeekMode,
    math::duration_to_float,
    source, Float, Source,
};
use crate::{decoder::builder::Settings, BitDepth};

/// Determines if a codec has stable parameters throughout the stream.
#[inline]
fn has_stable_parameters(codec: CodecType) -> bool {
    matches!(
        codec,
        CODEC_TYPE_FLAC
            | CODEC_TYPE_ALAC
            | CODEC_TYPE_WAVPACK
            | CODEC_TYPE_TTA
            | CODEC_TYPE_MONKEYS_AUDIO
            | CODEC_TYPE_PCM_S8
            | CODEC_TYPE_PCM_S16LE
            | CODEC_TYPE_PCM_S16BE
            | CODEC_TYPE_PCM_S24LE
            | CODEC_TYPE_PCM_S24BE
            | CODEC_TYPE_PCM_S32LE
            | CODEC_TYPE_PCM_S32BE
            | CODEC_TYPE_PCM_U8
            | CODEC_TYPE_PCM_U16LE
            | CODEC_TYPE_PCM_U16BE
            | CODEC_TYPE_PCM_U24LE
            | CODEC_TYPE_PCM_U24BE
            | CODEC_TYPE_PCM_U32LE
            | CODEC_TYPE_PCM_U32BE
            | CODEC_TYPE_PCM_F32LE
            | CODEC_TYPE_PCM_F32BE
            | CODEC_TYPE_PCM_F64LE
            | CODEC_TYPE_PCM_F64BE
            | CODEC_TYPE_PCM_ALAW
            | CODEC_TYPE_PCM_MULAW
    )
}

/// A wrapper around a `Read + Seek` type that implements Symphonia's `MediaSource` trait.
///
/// This adapter enables standard Rust I/O types to be used with Symphonia's media framework
/// by bridging the gap between Rust's I/O traits and Symphonia's requirements. It provides
/// stream metadata while delegating actual I/O operations to the wrapped type.
///
/// # Use Cases
///
/// - **File decoding**: Wrapping `std::fs::File` for audio file processing
/// - **Memory streams**: Adapting `std::io::Cursor<Vec<u8>>` for in-memory audio
/// - **Network streams**: Enabling seekable network streams with known lengths
/// - **Custom sources**: Integrating any `Read + Seek` implementation
///
/// # Metadata Handling
///
/// The wrapper provides Symphonia with essential stream characteristics:
/// - **Seekability**: Whether random access operations are supported
/// - **Byte length**: Total stream size for seeking and progress calculations
/// - **Configuration**: Stream properties from decoder builder settings
///
/// # Generic Parameters
///
/// * `T` - The wrapped I/O type, must implement `Read + Seek + Send + Sync`
pub struct ReadSeekSource<T: Read + Seek + Send + Sync> {
    /// The wrapped reader/seeker that provides actual I/O operations.
    ///
    /// All read and seek operations are delegated directly to this inner type,
    /// ensuring that performance characteristics are preserved.
    inner: T,

    /// Optional length of the media source in bytes.
    ///
    /// When known, this enables several optimizations:
    /// - **Seeking calculations**: Supports percentage-based and end-relative seeks
    /// - **Duration estimation**: Helps estimate playback duration for some formats
    /// - **Progress tracking**: Enables accurate progress indication
    /// - **Buffer management**: Assists with memory allocation decisions
    ///
    /// This value comes from the decoder settings and should represent the
    /// exact byte length of the audio stream.
    byte_len: Option<u64>,

    /// Whether this media source reports as seekable to Symphonia.
    ///
    /// This flag controls Symphonia's seeking behavior and optimization decisions:
    /// - **`true`**: Enables random access seeking operations
    /// - **`false`**: Restricts to forward-only streaming operations
    ///
    /// The flag should accurately reflect the underlying stream's capabilities.
    /// Incorrect values may lead to seek failures or suboptimal performance.
    is_seekable: bool,
}

impl<T: Read + Seek + Send + Sync> ReadSeekSource<T> {
    /// Creates a new `ReadSeekSource` by wrapping a reader/seeker.
    ///
    /// This constructor extracts relevant configuration from decoder settings
    /// to provide Symphonia with appropriate stream metadata while preserving
    /// the original I/O source's functionality.
    ///
    /// # Arguments
    ///
    /// * `inner` - The reader/seeker to wrap (takes ownership)
    /// * `settings` - Decoder settings containing stream metadata
    ///
    /// # Performance
    ///
    /// This operation is very lightweight, involving only metadata copying
    /// and ownership transfer. No I/O operations are performed.
    #[inline]
    pub fn new(inner: T, settings: &Settings) -> Self {
        ReadSeekSource {
            inner,
            byte_len: settings.byte_len,
            is_seekable: settings.is_seekable,
        }
    }
}

impl<T: Read + Seek + Send + Sync> MediaSource for ReadSeekSource<T> {
    /// Returns whether this media source supports random access seeking.
    ///
    /// This value is determined from the decoder settings and should accurately
    /// reflect the underlying stream's capabilities. Symphonia uses this information
    /// to decide whether to attempt seeking operations or restrict to forward-only access.
    ///
    /// # Returns
    ///
    /// - `true` if random access seeking is supported
    /// - `false` if only forward access is available
    ///
    /// # Impact on Symphonia
    ///
    /// When `false`, Symphonia may:
    /// - Avoid backward seeking operations
    /// - Provide degraded seeking functionality
    #[inline]
    fn is_seekable(&self) -> bool {
        self.is_seekable
    }

    /// Returns the total length of the media source in bytes, if known.
    ///
    /// This length information enables various Symphonia optimizations including
    /// seeking calculations, progress indication, and memory management decisions.
    /// The value should represent the exact byte length of the audio stream.
    ///
    /// # Returns
    ///
    /// - `Some(length)` if the total byte length is known
    /// - `None` if the length cannot be determined
    ///
    /// # Usage by Symphonia
    ///
    /// Symphonia may use this information for:
    /// - **Seeking calculations**: Computing byte offsets for time-based seeks
    /// - **Format detection**: Some formats benefit from knowing stream length
    ///
    /// # Accuracy Requirements
    ///
    /// The returned length must be accurate, as incorrect values may cause:
    /// - Seeking errors or failures
    /// - Incorrect duration calculations
    /// - Progress indication inaccuracies
    #[inline]
    fn byte_len(&self) -> Option<u64> {
        self.byte_len
    }
}

impl<T: Read + Seek + Send + Sync> Read for ReadSeekSource<T> {
    /// Reads bytes from the underlying reader into the provided buffer.
    ///
    /// This method provides a zero-cost delegation to the wrapped reader's
    /// implementation, preserving all performance characteristics and behavior
    /// of the original I/O source.
    ///
    /// # Arguments
    ///
    /// * `buf` - Buffer to read data into
    ///
    /// # Returns
    ///
    /// - `Ok(n)` where `n` is the number of bytes read
    /// - `Err(error)` if an I/O error occurred
    ///
    /// # Behavior
    ///
    /// The behavior is identical to the wrapped type's `read` implementation:
    /// - May read fewer bytes than requested
    /// - Returns 0 when end of stream is reached
    /// - May block if the underlying source blocks
    /// - Preserves all error conditions from the wrapped source
    ///
    /// # Performance
    ///
    /// This delegation has zero overhead and maintains the performance
    /// characteristics of the underlying I/O implementation.
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<T: Read + Seek + Send + Sync> Seek for ReadSeekSource<T> {
    /// Seeks to a position in the underlying reader.
    ///
    /// This method provides a zero-cost delegation to the wrapped reader's
    /// seek implementation, preserving all seeking behavior and performance
    /// characteristics of the original I/O source.
    ///
    /// # Arguments
    ///
    /// * `pos` - The position to seek to, relative to various points in the stream
    ///
    /// # Returns
    ///
    /// - `Ok(position)` - The new absolute position from the start of the stream
    /// - `Err(error)` - If a seek error occurred
    ///
    /// # Behavior
    ///
    /// The behavior is identical to the wrapped type's `seek` implementation:
    /// - Supports all `SeekFrom` variants (Start, End, Current)
    /// - May fail if the underlying source doesn't support seeking
    /// - Preserves all error conditions from the wrapped source
    /// - Updates the stream position for subsequent read operations
    ///
    /// # Performance
    ///
    /// This delegation has zero overhead and maintains the seeking performance
    /// characteristics of the underlying I/O implementation.
    ///
    /// # Thread Safety
    ///
    /// Seeking operations are not automatically synchronized. If multiple threads
    /// access the same source, external synchronization is required.
    #[inline]
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// Multi-format audio decoder using the Symphonia library.
///
/// This decoder provides comprehensive audio format support through Symphonia's
/// pluggable codec and container architecture. It automatically detects formats,
/// selects appropriate tracks, and handles complex features like multi-track files
/// and codec parameter changes.
///
/// # Architecture
///
/// The decoder consists of several key components:
/// - **Format reader/demuxer**: Parses container formats and extracts packets
/// - **Codec decoder**: Decodes audio packets to PCM samples
/// - **Sample buffer**: Holds decoded audio data for iteration
/// - **Track selection**: Automatically selects the first audio track
///
/// # Multi-track Support
///
/// For files with multiple tracks, the decoder automatically selects the first
/// track with a supported audio codec. When codec resets occur (rare), it
/// attempts to continue with the next available audio track.
///
/// # Buffer Management
///
/// The decoder uses dynamic buffer allocation based on codec capabilities:
/// - Buffer size determined by maximum frame length for the codec
/// - Buffers reused when possible to minimize allocations
/// - Automatic buffer clearing on decode errors for robust operation
///
/// # Error Recovery
///
/// The decoder implements sophisticated error recovery:
/// - **Decode errors**: Skip corrupted packets and continue
/// - **Reset required**: Recreate decoder and continue with next track
/// - **I/O errors**: Attempt to continue when possible
/// - **Terminal errors**: Clean shutdown when recovery is impossible
pub struct SymphoniaDecoder {
    /// The underlying Symphonia audio decoder.
    ///
    /// Handles the actual audio decoding from compressed packets to PCM samples.
    /// May be recreated during playback if codec parameters change or reset is required.
    decoder: Box<dyn Decoder>,

    /// Current position within the decoded audio buffer.
    ///
    /// Tracks the next sample index to return from the current buffer.
    /// Reset to 0 when a new packet is decoded and buffered.
    current_span_offset: usize,

    /// The format reader/demuxer for the container.
    ///
    /// Responsible for parsing the container format and extracting audio packets.
    /// Different implementations exist for each supported container format.
    demuxer: Box<dyn FormatReader>,

    /// Total duration from track metadata.
    ///
    /// Calculated from track timebase and frame count when available.
    /// May be `None` for streams without duration metadata or live streams.
    total_duration: Option<Duration>,

    /// Sample rate of the audio stream.
    sample_rate: SampleRate,

    /// Number of audio channels.
    channels: ChannelCount,

    /// Bit depth of the audio samples.
    bits_per_sample: Option<BitDepth>,

    /// Current decoded audio buffer.
    ///
    /// Contains interleaved PCM samples from the most recently decoded packet.
    /// `None` indicates that a new packet needs to be decoded.
    buffer: Option<SampleBuffer<Sample>>,

    /// Seeking precision mode.
    ///
    /// Controls the trade-off between seeking speed and accuracy:
    /// - `Fastest`: Uses coarse seeking (faster, less accurate)
    /// - `Nearest`: Uses precise seeking (slower, sample-accurate)
    seek_mode: SeekMode,

    /// Total number of samples (estimated from duration).
    ///
    /// Calculated from frame count when available, otherwise from duration.
    /// Used for progress indication and size hints.
    total_samples: Option<u64>,

    /// Number of samples read so far.
    ///
    /// Tracks current playback position for seeking calculations and progress indication.
    /// Updated on every sample returned from the iterator.
    samples_read: u64,

    /// ID of the currently selected track.
    ///
    /// Used to filter packets and handle track changes during playback.
    /// May change if codec reset occurs and track switching is needed.
    track_id: u32,

    /// Whether seeking operations are supported.
    ///
    /// Determined by the underlying media source stream capabilities.
    /// Required for backward seeking in most formats.
    is_seekable: bool,

    /// Total byte length of the source (for seeking calculations).
    ///
    /// Required for some seeking operations, particularly coarse seeking in MP3.
    /// When not available, seeking may fall back to less optimal methods.
    byte_len: Option<u64>,

    /// Whether audio parameters remain stable throughout the entire stream.
    ///
    /// Used to optimize performance by avoiding unnecessary checks for parameter changes.
    has_stable_parameters: bool,
}

impl SymphoniaDecoder {
    /// Creates a Symphonia decoder with default settings.
    ///
    /// This method initializes the decoder with default configuration, which includes
    /// no format hints, disabled gapless playback, and nearest seeking mode. For better
    /// performance and functionality, consider using `new_with_settings`.
    ///
    /// # Arguments
    ///
    /// * `mss` - MediaSourceStream containing the audio data
    ///
    /// # Returns
    ///
    /// - `Ok(SymphoniaDecoder)` if initialization succeeded
    /// - `Err(DecoderError)` if format detection or initialization failed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use rodio::decoder::symphonia::SymphoniaDecoder;
    /// use symphonia::core::io::MediaSourceStream;
    /// use std::fs::File;
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let mss = MediaSourceStream::new(Box::new(file), Default::default());
    /// let decoder = SymphoniaDecoder::new(mss).unwrap();
    /// ```
    ///
    /// # Performance
    ///
    /// Without format hints, the decoder must probe all supported formats,
    /// which can be slower than providing hints via `new_with_settings`.
    #[allow(dead_code)]
    pub fn new(mss: MediaSourceStream) -> Result<Self, DecoderError> {
        Self::new_with_settings(mss, &Settings::default())
    }

    /// Creates a Symphonia decoder with custom settings.
    ///
    /// This method provides full control over decoder initialization, including format hints,
    /// seeking configuration, and performance optimizations. It performs format detection,
    /// track selection, and initial packet decoding to establish stream characteristics.
    ///
    /// # Arguments
    ///
    /// * `mss` - MediaSourceStream containing the audio data
    /// * `settings` - Configuration settings from `DecoderBuilder`
    ///
    /// # Returns
    ///
    /// - `Ok(SymphoniaDecoder)` if initialization succeeded
    /// - `Err(DecoderError)` if format detection or initialization failed
    ///
    /// # Settings Usage
    ///
    /// - `hint`: Format hint (e.g., "mp3", "m4a") for faster detection
    /// - `mime_type`: MIME type hint for format identification
    /// - `gapless`: Enable gapless playback for supported formats
    /// - `seek_mode`: Control seeking precision vs. speed trade-off
    /// - Additional settings affect seeking and performance behavior
    ///
    /// # Error Handling
    ///
    /// Various initialization errors are mapped to appropriate `DecoderError` variants:
    /// - `UnrecognizedFormat`: No suitable format found
    /// - `NoStreams`: File contains no audio tracks
    /// - `IoError`: I/O error during initialization
    /// - `DecodeError`: Error decoding initial packet
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use rodio::decoder::{symphonia::SymphoniaDecoder, Settings, builder::SeekMode};
    /// use symphonia::core::io::MediaSourceStream;
    /// use std::fs::File;
    ///
    /// let file = File::open("audio.m4a").unwrap();
    /// let mss = MediaSourceStream::new(Box::new(file), Default::default());
    ///
    /// let mut settings = Settings::default();
    /// settings.hint = Some("m4a".to_string());
    /// settings.gapless = true;
    /// settings.seek_mode = SeekMode::Fastest;
    ///
    /// let decoder = SymphoniaDecoder::new_with_settings(mss, &settings).unwrap();
    /// ```
    ///
    /// # Performance
    ///
    /// Providing accurate format hints significantly improves initialization speed
    /// by reducing the number of format probes required.
    pub fn new_with_settings(
        mss: MediaSourceStream,
        settings: &Settings,
    ) -> Result<Self, DecoderError> {
        match SymphoniaDecoder::init(mss, settings) {
            Err(e) => match e {
                Error::IoError(e) => Err(DecoderError::IoError(e.to_string())),
                Error::DecodeError(e) => Err(DecoderError::DecodeError(e)),
                Error::SeekError(_) => {
                    unreachable!("Seek errors should not occur during initialization")
                }
                Error::Unsupported(_) => Err(DecoderError::UnrecognizedFormat),
                Error::LimitError(e) => Err(DecoderError::LimitError(e)),
                Error::ResetRequired => Err(DecoderError::ResetRequired),
            },
            Ok(Some(decoder)) => Ok(decoder),
            Ok(None) => Err(DecoderError::NoStreams),
        }
    }

    /// Consumes the decoder and returns the underlying media source stream.
    ///
    /// This can be useful for recovering the original data source after decoding
    /// is complete or when the decoder needs to be replaced. The stream will be
    /// positioned at the current playback location.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use rodio::decoder::symphonia::SymphoniaDecoder;
    /// use std::fs::File;
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let mss = MediaSourceStream::new(Box::new(file), Default::default());
    /// let decoder = SymphoniaDecoder::new(mss).unwrap();
    /// let recovered_mss = decoder.into_inner();
    /// ```
    ///
    /// # Stream Position
    ///
    /// The returned MediaSourceStream will be positioned at the current
    /// packet location, which may be useful for advanced processing or
    /// manual demuxing operations.
    #[inline]
    pub fn into_inner(self) -> MediaSourceStream {
        self.demuxer.into_inner()
    }

    /// Initializes the Symphonia decoder with format detection and track selection.
    ///
    /// This internal method handles the complex initialization process including:
    /// format probing, track selection, decoder creation, and initial packet decoding
    /// to establish stream characteristics. It implements robust error handling
    /// for various initialization scenarios.
    ///
    /// # Arguments
    ///
    /// * `mss` - MediaSourceStream containing the audio data
    /// * `settings` - Configuration settings affecting initialization
    ///
    /// # Returns
    ///
    /// - `Ok(Some(decoder))` if initialization succeeded
    /// - `Ok(None)` if no suitable audio tracks found
    /// - `Err(Error)` if format detection or initialization failed
    ///
    /// # Initialization Process
    ///
    /// 1. **Format probing**: Detect container format using hints and probing
    /// 2. **Track selection**: Find first track with supported audio codec
    /// 3. **Decoder creation**: Create appropriate codec decoder for track
    /// 4. **Initial decode**: Decode first packet to establish signal specification
    /// 5. **Buffer allocation**: Allocate sample buffer based on codec capabilities
    /// 6. **Duration calculation**: Calculate total duration from metadata
    ///
    /// # Error Recovery
    ///
    /// The method handles various error conditions during initialization:
    /// - **Reset required**: Recreates decoder and continues
    /// - **Decode errors**: Skips corrupted packets during initialization
    /// - **Empty packets**: Continues searching for valid audio data
    /// - **Track changes**: Adapts to multi-track scenarios
    ///
    /// # Performance Optimizations
    ///
    /// - Uses format hints to reduce probing overhead
    /// - Allocates buffers based on codec maximum frame size
    /// - Caches duration and sample count calculations
    /// - Reuses existing allocations when possible
    fn init(
        mss: MediaSourceStream,
        settings: &Settings,
    ) -> symphonia::core::errors::Result<Option<SymphoniaDecoder>> {
        let mut hint = Hint::new();
        if let Some(ext) = settings.hint.as_ref() {
            hint.with_extension(ext);
        }
        if let Some(typ) = settings.mime_type.as_ref() {
            hint.mime_type(typ);
        }
        let format_opts: FormatOptions = FormatOptions {
            enable_gapless: settings.gapless,
            ..Default::default()
        };
        let metadata_opts: MetadataOptions = Default::default();
        let is_seekable = mss.is_seekable();
        let byte_len = mss.byte_len();

        // Select the first supported track (non-null codec)
        let mut probed = get_probe().format(&hint, mss, &format_opts, &metadata_opts)?;
        let track = probed
            .format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or(Error::Unsupported("No track with supported codec"))?;

        let mut track_id = track.id;
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())?;
        let total_duration: Option<Duration> = track
            .codec_params
            .time_base
            .zip(track.codec_params.n_frames)
            .map(|(base, spans)| base.calc_time(spans).into());

        // Find the first decodable packet and initialize spec from it
        let (spec, buffer) = loop {
            let current_span = match probed.format.next_packet() {
                Ok(packet) => packet,

                // If ResetRequired is returned, then the track list must be re-examined and all
                // Decoders re-created.
                Err(Error::ResetRequired) => {
                    track_id = recreate_decoder(&mut probed.format, &mut decoder, None)?;
                    continue;
                }

                // All other errors are unrecoverable.
                Err(e) => return Err(e),
            };

            // If the packet does not belong to the selected track, skip over it
            if current_span.track_id() != track_id {
                continue;
            }

            match decoder.decode(&current_span) {
                Ok(decoded) => {
                    // Only accept packets with actual audio frames
                    if decoded.frames() > 0 {
                        // Set spec from first successful decode
                        let spec = decoded.spec().to_owned();

                        // Allocate buffer based on maximum frame length for codec
                        let mut sample_buffer =
                            SampleBuffer::<Sample>::new(decoded.capacity() as u64, *decoded.spec());
                        sample_buffer.copy_interleaved_ref(decoded);
                        let buffer = Some(sample_buffer);
                        break (spec, buffer);
                    }
                    continue; // Empty packet - try the next one
                }
                Err(e) => {
                    if should_continue_on_decode_error(&e, &mut decoder) {
                        continue;
                    } else {
                        return Err(e);
                    }
                }
            }
        };

        // Cache initial spec values
        let sample_rate = SampleRate::new(spec.rate).expect("Invalid sample rate");
        let channels = spec
            .channels
            .count()
            .try_into()
            .ok()
            .and_then(ChannelCount::new)
            .expect("Invalid channel count");
        let bits_per_sample = decoder
            .codec_params()
            .bits_per_sample
            .and_then(BitDepth::new);

        // Calculate total samples
        let total_samples = {
            // Try frame-based calculation first (most accurate)
            if let (Some(n_frames), Some(max_frame_length)) = (
                decoder.codec_params().n_frames,
                decoder.codec_params().max_frames_per_packet,
            ) {
                n_frames.checked_mul(max_frame_length)
            } else if let Some(duration) = total_duration {
                // Fallback to duration-based calculation
                let total_secs = duration_to_float(duration);
                Some(
                    (total_secs * sample_rate.get() as Float * channels.get() as Float).ceil()
                        as u64,
                )
            } else {
                None
            }
        };

        let has_stable_parameters = settings.stable_parameters
            || (has_stable_parameters(decoder.codec_params().codec)
                && probed.format.tracks().len() == 1);

        Ok(Some(Self {
            decoder,
            current_span_offset: 0,
            demuxer: probed.format,
            total_duration,
            sample_rate,
            channels,
            bits_per_sample,
            buffer,
            seek_mode: settings.seek_mode,
            total_samples,
            samples_read: 0,
            track_id,
            is_seekable,
            byte_len,
            has_stable_parameters,
        }))
    }

    /// Parses the signal specification from the decoder and returns sample rate, channel count,
    /// and bit depth.
    fn cache_spec(&mut self) {
        if let Some(rate) = self.decoder.codec_params().sample_rate {
            if let Some(rate) = SampleRate::new(rate) {
                self.sample_rate = rate;
            }
        }

        if let Some(channels) = self.decoder.codec_params().channels {
            if let Some(count) = channels.count().try_into().ok().and_then(ChannelCount::new) {
                self.channels = count;
            }
        }

        if let Some(bits_per_sample) = self.decoder.codec_params().bits_per_sample {
            self.bits_per_sample = BitDepth::new(bits_per_sample);
        }
    }
}

impl Source for SymphoniaDecoder {
    /// Returns the number of samples before parameters change.
    ///
    /// # Parameter Stability Optimization
    ///
    /// For streams with guaranteed stable parameters (single-track files using codecs
    /// like FLAC, WAV/PCM, ALAC), this returns `None` to indicate unlimited stability.
    /// This allows downstream processing to optimize by avoiding parameter checks on
    /// every sample.
    ///
    /// # Parameter Changes
    ///
    /// For unstable streams, Symphonia may encounter parameter changes:
    /// - **Track switching**: Multi-track files (.mkv, .mp4) with different specifications
    /// - **Chained streams**: Concatenated streams (Ogg) with different parameters
    /// - **Codec resets**: Mid-stream parameter changes (rare)
    ///
    /// # Buffer Sizes
    ///
    /// When returned, buffer sizes are determined by the codec's maximum frame length
    /// and may vary between packets based on encoding complexity.
    ///
    /// # Returns
    ///
    /// - `None` for streams with guaranteed stable parameters (optimization hint)
    /// - `Some(n)` with current buffer length for potentially unstable streams
    /// - `Some(0)` when the stream is exhausted
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        if self.has_stable_parameters {
            None
        } else {
            // Parameters may change - return buffer length to force rechecks after each buffer
            self.buffer.as_ref().map(SampleBuffer::len).or(Some(0))
        }
    }

    /// Returns the number of audio channels.
    ///
    /// # Dynamic Changes
    ///
    /// While most files have consistent channel configuration, Symphonia handles
    /// cases where channel count may change:
    /// - **Multi-track files**: Different tracks with different channel counts
    /// - **Codec resets**: Parameter changes requiring decoder recreation
    /// - **Chained streams**: Concatenated streams with different specifications
    ///
    /// # Channel Mapping
    ///
    /// Symphonia follows format-specific channel mapping conventions, which
    /// vary between container formats and codecs. The decoder preserves the
    /// original channel order from the source material.
    ///
    /// # Guarantees
    ///
    /// The returned value reflects the current signal specification and is
    /// valid for the current buffer. It may change between buffers if stream
    /// parameters change.
    #[inline]
    fn channels(&self) -> ChannelCount {
        self.channels
    }

    /// Returns the sample rate in Hz.
    ///
    /// # Dynamic Changes
    ///
    /// While most files have consistent sample rates, Symphonia handles cases
    /// where sample rate may change during playback:
    /// - **Multi-track files**: Different tracks with different sample rates
    /// - **Codec resets**: Parameter changes requiring decoder recreation
    /// - **Chained streams**: Concatenated streams with different sample rates
    ///
    /// # Guarantees
    ///
    /// The returned value reflects the current signal specification and is
    /// valid for the current buffer. It may change between buffers if stream
    /// parameters change.
    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    /// Returns the total duration of the audio stream.
    ///
    /// # Availability
    ///
    /// Duration is available when:
    /// 1. Track contains timebase and frame count metadata
    /// 2. Container format provides duration information
    /// 3. Format reader successfully extracts timing metadata
    ///
    /// # Multi-track Files
    ///
    /// For multi-track files, duration represents the length of the currently
    /// selected audio track, not the entire file duration.
    ///
    /// # Returns
    ///
    /// Returns `None` for:
    /// - Live streams without predetermined duration
    /// - Malformed files missing duration metadata
    /// - Streams where duration cannot be determined
    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }

    /// Returns the bit depth of the audio samples.
    ///
    /// # Format Support
    ///
    /// For lossy formats, bit depth is not meaningful as the audio undergoes
    /// compression that removes the original bit depth information. Lossless
    /// formats preserve and report the original bit depth.
    ///
    /// # Implementation Note
    ///
    /// Up to 24 bits of information is preserved from the original stream and
    /// used for proper sample scaling during conversion to Rodio's sample format.
    ///
    /// # Returns
    ///
    /// - `Some(depth)` for formats that preserve bit depth information
    /// - `None` for lossy formats or when bit depth is not determinable
    #[inline]
    fn bits_per_sample(&self) -> Option<BitDepth> {
        self.bits_per_sample
    }

    /// Attempts to seek to the specified position in the audio stream.
    ///
    /// Symphonia seeking behavior varies significantly by format and configuration.
    /// The implementation provides both coarse (fast) and accurate (precise) seeking
    /// modes, with automatic fallbacks for optimal compatibility.
    ///
    /// # Seeking Modes
    ///
    /// - **`SeekMode::Fastest`**: Uses coarse seeking when possible
    ///   - Faster performance with larger tolerances
    ///   - May require fine-tuning for exact positioning
    ///   - Falls back to accurate seeking if byte length unavailable
    /// - **`SeekMode::Nearest`**: Uses accurate seeking for precision
    ///   - Sample-accurate positioning when supported
    ///   - Slower performance due to precise calculations
    ///   - Always attempts exact positioning
    ///
    /// # Format-Specific Behavior
    ///
    /// Different formats have varying seeking requirements and capabilities:
    ///
    /// | Format | Direction | Mode | Requirements | Notes |
    /// |--------|-----------|------|--------------|-------|
    /// | AAC    | Backward  | Any  | is_seekable  | Standard behavior |
    /// | FLAC   | Backward  | Any  | is_seekable  | Reliable seeking |
    /// | MP3    | Backward  | Any  | is_seekable  | Good compatibility |
    /// | MP3    | Any       | Coarse | byte_len   | Unique requirement |
    /// | MP4    | Backward  | Any  | is_seekable  | No auto fallback |
    /// | OGG    | Any       | Any  | is_seekable  | May fail silently |
    /// | WAV    | Backward  | Any  | is_seekable  | Excellent performance |
    ///
    /// # Performance Characteristics
    ///
    /// - **Coarse seeks**: O(log n) performance for most formats
    /// - **Accurate seeks**: Variable performance, format-dependent
    /// - **Forward seeks**: Often optimized by skipping packets
    /// - **Backward seeks**: Require stream reset and repositioning
    ///
    /// # Error Handling
    ///
    /// The method handles various seeking scenarios:
    /// - **Forward-only mode**: Prevents backward seeks when not seekable
    /// - **Vorbis workaround**: Uses linear seeking for problematic streams
    /// - **Automatic fallbacks**: Switches seek modes when needed
    /// - **Boundary clamping**: Limits seeks to valid stream range
    ///
    /// # Arguments
    ///
    /// * `pos` - Target position as duration from stream start
    ///
    /// # Errors
    ///
    /// - `SeekError::ForwardOnly` - Backward seek attempted without seekable flag
    /// - `SeekError::Demuxer` - Underlying demuxer error during seek operation
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::{fs::File, time::Duration};
    /// use rodio::{Decoder, Source, decoder::builder::SeekMode};
    ///
    /// let file = File::open("audio.m4a").unwrap();
    /// let mut decoder = Decoder::builder()
    ///     .with_data(file)
    ///     .with_seekable(true)
    ///     .with_seek_mode(SeekMode::Fastest)
    ///     .build()
    ///     .unwrap();
    ///
    /// // Fast seek to 30 seconds
    /// decoder.try_seek(Duration::from_secs(30)).unwrap();
    /// ```
    ///
    /// # Implementation Details
    ///
    /// The seeking implementation includes several optimizations:
    /// - **Channel preservation**: Maintains correct channel alignment after seeks
    /// - **Decoder reset**: Ensures clean state after demuxer seeks
    /// - **Position tracking**: Updates sample counters based on actual seek results
    /// - **Fine-tuning**: Sample-accurate positioning when precise mode is used
    fn try_seek(&mut self, pos: Duration) -> Result<(), source::SeekError> {
        // Seeking should be "saturating", meaning: target positions beyond the end of the stream
        // are clamped to the end.
        let mut target = pos;
        if let Some(total_duration) = self.total_duration() {
            if target > total_duration {
                target = total_duration;
            }
        }

        let target_samples = (duration_to_float(target)
            * self.sample_rate().get() as Float
            * self.channels().get() as Float) as u64;

        // Remember the current channel, so we can restore it after seeking.
        let active_channel = self.current_span_offset % self.channels().get() as usize;

        // | Format | Direction | SymphoniaSeekMode | Requires           | Remarks               |
        // |--------|-----------|-------------------|--------------------|-----------------------|
        // | AAC    | Backward  | Any               | is_seekable        |                       |
        // | AIFF   | Backward  | Any               | is_seekable        |                       |
        // | CAF    | Backward  | Any               | is_seekable        |                       |
        // | FLAC   | Backward  | Any               | is_seekable        |                       |
        // | MKV    | Backward  | Any               | is_seekable        |                       |
        // | MP3    | Backward  | Any               | is_seekable        |                       |
        // | MP3    | Any       | Coarse            | byte_len.is_some() | No other coarse impls |
        // | MP4    | Backward  | Any               | is_seekable        | No automatic fallback |
        // | OGG    | Any       | Any               | is_seekable        | Fails silently if not |
        // | WAV    | Backward  | Any               | is_seekable        |                       |
        if !self.is_seekable {
            if target_samples < self.samples_read {
                return Err(source::SeekError::ForwardOnly);
            }

            // TODO: remove when Symphonia has fixed linear seeking for Vorbis
            if self.decoder.codec_params().codec == CODEC_TYPE_VORBIS {
                for _ in self.samples_read..target_samples {
                    let _ = self.next();
                }
                return Ok(());
            }
        }

        let seek_mode = if self.seek_mode == SeekMode::Fastest && self.byte_len.is_none() {
            // Fallback to accurate (nearest) seeking if no byte length is known
            SymphoniaSeekMode::Accurate
        } else {
            self.seek_mode.into()
        };

        let seek_res = self
            .demuxer
            .seek(
                seek_mode,
                SeekTo::Time {
                    time: target.into(),
                    track_id: Some(self.track_id),
                },
            )
            .map_err(Arc::new)?;

        // Seeking is a demuxer operation without the decoder knowing about it, so we need to reset
        // the decoder to make sure it's in sync and prevent audio glitches.
        self.decoder.reset();

        // Clear buffer - let next() handle loading new packets
        self.buffer = None;

        // Update samples_read counter based on actual seek position
        self.samples_read = if let Some(time_base) = self.decoder.codec_params().time_base {
            let actual_time = Duration::from(time_base.calc_time(seek_res.actual_ts));
            (duration_to_float(actual_time)
                * self.sample_rate().get() as Float
                * self.channels().get() as Float) as u64
        } else {
            // Fallback in the unexpected case that the format has no base time set
            seek_res.actual_ts * self.sample_rate().get() as u64 * self.channels().get() as u64
        };

        // Symphonia does not seek to the exact position, it seeks to the closest keyframe.
        // If nearest seeking is required, fast-forward to the exact position.
        let mut samples_to_skip = 0;
        if self.seek_mode == SeekMode::Nearest {
            // Calculate the number of samples to skip.
            samples_to_skip = (Duration::from(
                self.decoder
                    .codec_params()
                    .time_base
                    .expect("time base availability guaranteed by caller")
                    .calc_time(seek_res.required_ts.saturating_sub(seek_res.actual_ts)),
            )
            .as_secs_f32()
                * self.sample_rate().get() as f32
                * self.channels().get() as f32)
                .ceil() as usize;

            // Re-align the seek position to the first channel.
            samples_to_skip -= samples_to_skip % self.channels().get() as usize
        };

        // After seeking, we are at the beginning of an inter-sample frame, i.e. the first channel.
        // We need to advance the iterator to the right channel.
        for _ in 0..(samples_to_skip + active_channel) {
            let _ = self.next();
        }

        Ok(())
    }
}

impl Iterator for SymphoniaDecoder {
    /// The type of samples yielded by the iterator.
    ///
    /// Returns `Sample` values representing individual audio samples. Samples are interleaved
    /// across channels in the order determined by the format's channel mapping specification.
    type Item = Sample;

    /// Returns the next audio sample from the multi-format stream.
    ///
    /// This method implements packet-based decoding with robust error recovery.
    /// It automatically handles format-specific details, codec resets, track changes,
    /// and various error conditions while maintaining optimal performance.
    ///
    /// # Decoding Process
    ///
    /// The method follows a two-phase approach:
    /// 1. **Hot path**: Return samples from current buffer (very fast)
    /// 2. **Cold path**: Decode new packets when buffer is exhausted (slower)
    ///
    /// # Buffer Management
    ///
    /// The decoder uses intelligent buffer management:
    /// - **Reuse existing buffers**: Minimizes allocations during playback
    /// - **Dynamic allocation**: Creates buffers based on codec capabilities
    /// - **Capacity-based sizing**: Uses maximum frame length for optimal performance
    /// - **Automatic clearing**: Handles error conditions gracefully
    ///
    /// # Error Recovery
    ///
    /// The method implements comprehensive error recovery:
    /// - **Decode errors**: Skip corrupted packets and continue playback
    /// - **Reset required**: Recreate decoder and attempt track switching
    /// - **I/O errors**: Attempt continuation when possible
    /// - **Empty packets**: Skip metadata-only packets automatically
    /// - **Track changes**: Handle multi-track scenarios transparently
    ///
    /// # Performance Optimizations
    ///
    /// - **Buffer reuse**: Minimizes memory allocations
    /// - **Error classification**: Quick decisions on error handling
    /// - **Packet filtering**: Efficient track-specific packet processing
    /// - **Lazy allocation**: Buffers created only when needed
    ///
    /// # Format Adaptation
    ///
    /// The decoder adapts to various format characteristics:
    /// - **Variable packet sizes**: Handles dynamic content-dependent sizes
    /// - **Codec changes**: Supports streams with changing codecs
    /// - **Multi-track streams**: Automatically filters relevant packets
    /// - **Parameter changes**: Adapts to changing signal specifications
    ///
    /// # Returns
    ///
    /// - `Some(sample)` - Next audio sample from the stream
    /// - `None` - End of stream reached or unrecoverable error occurred
    ///
    /// # Channel Order
    ///
    /// Samples are returned in format-specific channel order:
    /// - **WAV/PCM**: Standard channel mapping (L, R, C, LFE, ...)
    /// - **MP4/AAC**: AAC channel configuration standards
    /// - **OGG/Vorbis**: Vorbis channel mapping specification
    /// - **FLAC**: FLAC channel assignment standards
    fn next(&mut self) -> Option<Self::Item> {
        // Hot path: return sample from current buffer if available
        if let Some(buffer) = &self.buffer {
            if self.current_span_offset < buffer.len() {
                let sample = buffer.samples()[self.current_span_offset];
                self.current_span_offset += 1;
                self.samples_read += 1;
                return Some(sample);
            }
        }

        // Cold path: need to decode next packet
        let decoded = loop {
            let packet = match self.demuxer.next_packet() {
                Ok(packet) => packet,

                // If ResetRequired is returned, then the track list must be re-examined and all
                // Decoders re-created.
                Err(Error::ResetRequired) => {
                    self.track_id =
                        recreate_decoder(&mut self.demuxer, &mut self.decoder, Some(self.track_id))
                            .ok()?;
                    self.cache_spec();

                    // Clear buffer after decoder reset - spec may have been updated
                    self.buffer = None;
                    continue;
                }

                // All other errors are unrecoverable.
                Err(_) => return None,
            };

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    // Only accept packets with actual audio frames
                    if decoded.frames() > 0 {
                        break decoded;
                    }
                    continue; // Empty packet - try the next one
                }
                Err(e) => {
                    if should_continue_on_decode_error(&e, &mut self.decoder) {
                        // For recoverable errors, just clear buffer contents but keep allocation
                        if let Some(buffer) = self.buffer.as_mut() {
                            buffer.clear();
                        }
                        continue;
                    } else {
                        // Internal buffer *must* be cleared if an error occurs.
                        self.buffer = None;
                        return None; // Terminal error - end of iteration
                    }
                }
            }
        };

        // Reuse buffer when possible
        let buffer = match self.buffer.as_mut() {
            Some(buffer) => buffer,
            None => {
                // Although packet sizes are not guaranteed to be constant, the buffer
                // size is based on the maximum frame length for the codec, so we can
                // allocate once and reuse it for as long as the codec specifications
                // remain the same.
                self.buffer.insert(SampleBuffer::new(
                    decoded.capacity() as u64,
                    *decoded.spec(),
                ))
            }
        };
        buffer.copy_interleaved_ref(decoded);
        self.current_span_offset = 0;

        // Successfully fetched next packet
        if !buffer.is_empty() {
            // Buffer now has samples - return the first one. This is a bit redundant
            // but faster than calling next() recursively.
            let sample = buffer.samples()[0];
            self.current_span_offset = 1;
            self.samples_read += 1;
            Some(sample)
        } else {
            // Empty buffer after successful packet - could be that this packet contains metadata
            // only. Recursively try again until we hit the end of the stream.
            self.next()
        }
    }

    /// Returns bounds on the remaining amount of samples.
    ///
    /// Provides size estimates based on Symphonia's format analysis and current
    /// playback position. The accuracy depends on the availability and reliability
    /// of metadata from the underlying format.
    ///
    /// # Accuracy Levels
    ///
    /// - **High accuracy**: When total samples calculated from frame count metadata
    /// - **Moderate accuracy**: When estimated from duration and signal specification
    /// - **Conservative estimate**: When only current buffer information available
    /// - **Stream exhausted**: (0, Some(0)) when no more data
    ///
    /// # Format Variations
    ///
    /// Different formats provide varying levels of size information:
    /// - **FLAC**: Exact frame count in metadata (highest accuracy)
    /// - **WAV**: Sample count in header (highest accuracy)
    /// - **MP4**: Duration-based estimation (good accuracy)
    /// - **MP3**: Variable accuracy depending on encoding type
    /// - **OGG**: Duration-based when available
    ///
    /// # Implementation
    ///
    /// The lower bound represents samples currently buffered in memory.
    /// The upper bound uses the most accurate available method:
    /// 1. Frame-based calculation (when available)
    /// 2. Duration-based estimation (fallback)
    /// 3. No estimate (when insufficient information)
    ///
    /// # Use Cases
    ///
    /// - **Progress indication**: Upper bound enables percentage calculation
    /// - **Buffer allocation**: Lower bound ensures minimum available samples
    /// - **End detection**: (0, Some(0)) indicates stream completion
    /// - **Memory planning**: Helps optimize buffer sizes for processing
    ///
    /// # Multi-track Considerations
    ///
    /// For multi-track files, estimates represent the currently selected audio
    /// track, not the entire file duration or all tracks combined.
    fn size_hint(&self) -> (usize, Option<usize>) {
        // Samples already decoded and buffered (guaranteed available)
        let buffered_samples = self
            .current_span_len()
            .unwrap_or(0)
            .saturating_sub(self.current_span_offset);

        if let Some(total_samples) = self.total_samples {
            let total_remaining = total_samples.saturating_sub(self.samples_read) as usize;
            (buffered_samples, Some(total_remaining))
        } else if self.buffer.is_none() {
            // Stream exhausted
            (0, Some(0))
        } else {
            (buffered_samples, None)
        }
    }
}

/// Recreates decoder after ResetRequired error from format reader.
///
/// This function handles the complex process of decoder recreation when Symphonia
/// determines that stream parameters have changed significantly enough to require
/// a complete decoder reset. It implements intelligent track selection and error
/// recovery strategies.
///
/// # Arguments
///
/// * `format` - Mutable reference to the format reader/demuxer
/// * `decoder` - Mutable reference to the current decoder (will be replaced)
/// * `current_track_id` - Optional current track ID for track switching logic
/// * `spec` - Optional signal specification to update during recreation
///
/// # Track Selection Strategy
///
/// The function implements different strategies based on context:
/// - **Initialization**: Selects first supported track (current_track_id is None)
/// - **During playback**: Attempts to find next supported track after current one
/// - **No fallback during playback**: Prevents unexpected track jumping
///
/// # Error Handling
///
/// The function handles various error scenarios:
/// - **No supported tracks**: Returns appropriate error
/// - **Codec creation failure**: Propagates codec errors
/// - **Track not found**: Handles missing track scenarios
/// - **Specification updates**: Updates spec when track parameters available
///
/// # Returns
///
/// - `Ok(track_id)` - ID of the newly selected track
/// * `Err(Error)` - If no suitable track found or decoder creation failed
fn recreate_decoder(
    format: &mut Box<dyn FormatReader>,
    decoder: &mut Box<dyn Decoder>,
    current_track_id: Option<u32>,
) -> Result<u32, symphonia::core::errors::Error> {
    let track = if let Some(current_id) = current_track_id {
        // During playback: find the next supported track after the current one
        let tracks = format.tracks();
        let current_index = tracks.iter().position(|t| t.id == current_id);

        if let Some(idx) = current_index {
            // Look for the next supported track after current index
            tracks
                .iter()
                .skip(idx + 1)
                .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        } else {
            // Current track not found in tracks list
            None
        }
        // Note: No fallback during playback - if we can't find next track, stop playing
    } else {
        // Initialization case: find first supported track
        format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
    }
    .ok_or(Error::Unsupported(
        "No supported track found after current track",
    ))?;

    // Create new decoder
    *decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    Ok(track.id)
}

/// Determines whether to continue decoding after a decode error.
///
/// This function implements Symphonia's error handling recommendations,
/// classifying errors into recoverable and terminal categories. It enables
/// robust audio playback by gracefully handling common error conditions.
///
/// # Arguments
///
/// * `error` - The Symphonia error that occurred during decoding
/// * `decoder` - Mutable reference to the decoder (may be reset)
///
/// # Returns
///
/// - `true` if decoding should continue with the next packet
/// - `false` if the error is terminal and decoding should stop
fn should_continue_on_decode_error(
    error: &symphonia::core::errors::Error,
    decoder: &mut Box<dyn Decoder>,
) -> bool {
    match error {
        // If a `DecodeError` or `IoError` is returned, the packet is
        // undecodeable and should be discarded. Decoding may be continued
        // with the next packet.
        Error::DecodeError(_) | Error::IoError(_) => true,

        // If `ResetRequired` is returned, consumers of the decoded audio data
        // should expect the duration and `SignalSpec` of the decoded audio
        // buffer to change.
        Error::ResetRequired => {
            decoder.reset();
            true
        }

        // All other errors are unrecoverable.
        _ => false,
    }
}

/// Converts Rodio's SeekMode to Symphonia's SeekMode.
///
/// This conversion maps Rodio's seeking preferences to Symphonia's
/// internal seeking modes, enabling consistent seeking behavior
/// across different audio processing layers.
///
/// # Mapping
///
/// - `SeekMode::Fastest` → `SymphoniaSeekMode::Coarse`
///   - Prioritizes speed over precision
///   - Uses keyframe-based seeking when available
///   - Suitable for user scrubbing and fast navigation
/// - `SeekMode::Nearest` → `SymphoniaSeekMode::Accurate`
///   - Prioritizes precision over speed
///   - Attempts sample-accurate positioning
///   - Suitable for gapless playback and precise positioning
///
/// # Performance Implications
///
/// The choice between modes affects performance significantly:
/// - **Coarse**: Fast seeks but may require fine-tuning
/// - **Accurate**: Slower seeks but precise positioning
///
/// # Format Compatibility
///
/// Not all formats support both modes equally. Automatic
/// fallbacks may occur when preferred mode unavailable.
impl From<SeekMode> for SymphoniaSeekMode {
    fn from(mode: SeekMode) -> Self {
        match mode {
            SeekMode::Fastest => SymphoniaSeekMode::Coarse,
            SeekMode::Nearest => SymphoniaSeekMode::Accurate,
        }
    }
}
