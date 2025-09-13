//! Builder pattern for configuring and constructing audio decoders.
//!
//! This module provides a flexible builder API for creating decoders with custom settings
//! and optimizations. The builder pattern allows fine-grained control over decoder behavior,
//! performance characteristics, and feature enablement before decoder creation.
//!
//! # Architecture
//!
//! The builder system consists of three main components:
//! - **Settings**: Configuration container holding all decoder parameters
//! - **DecoderBuilder**: Fluent API for configuring settings and creating decoders
//! - **SeekMode**: Enum controlling seeking accuracy vs. speed trade-offs
//!
//! # Configuration Categories
//!
//! Settings are organized into several categories:
//! - **Format detection**: Hints and MIME types for faster format identification
//! - **Seeking behavior**: Seeking enablement, modes, and requirements
//! - **Performance**: Duration scanning, gapless playback, buffer management
//! - **Stream properties**: Byte length, seekability, duration information
//!
//! # Performance Optimization
//!
//! The builder enables several performance optimizations:
//! - **Format hints**: Reduce format detection overhead
//! - **Byte length**: Enable efficient seeking and duration calculation
//! - **Seek mode selection**: Balance speed vs. accuracy based on use case
//! - **Duration scanning**: Control expensive file analysis operations
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```no_run
//! use std::fs::File;
//! use rodio::Decoder;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let file = File::open("audio.mp3")?;
//!     let len = file.metadata()?.len();
//!
//!     let decoder = Decoder::builder()
//!         .with_data(file)
//!         .with_byte_len(len)      // Enable seeking and duration calculation
//!         .with_hint("mp3")        // Reduce format detection overhead
//!         .with_gapless(true)      // Enable gapless playback
//!         .build()?;
//!
//!     // Use the decoder...
//!     Ok(())
//! }
//! ```
//!
//! ## Advanced Configuration
//!
//! ```no_run
//! use std::fs::File;
//! use std::time::Duration;
//! use rodio::{Decoder, decoder::builder::SeekMode};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let file = File::open("audio.flac")?;
//!     let len = file.metadata()?.len();
//!
//!     // High-quality decoder with precise seeking
//!     let decoder = Decoder::builder()
//!         .with_data(file)
//!         .with_byte_len(len)
//!         .with_hint("flac")
//!         .with_mime_type("audio/flac")
//!         .with_seekable(true)
//!         .with_seek_mode(SeekMode::Nearest)
//!         .with_scan_duration(true)
//!         .with_gapless(false)
//!         .build()?;
//!
//!    Ok(())
//! }
//! ```
//!
//! # Configuration Reference
//!
//! The following settings can be configured:
//!
//! - **`byte_len`**: Total length of the input data in bytes (enables seeking/duration)
//! - **`hint`**: Format hint like "mp3", "wav", "flac" for faster detection
//! - **`mime_type`**: MIME type hint for container format identification
//! - **`seekable`**: Whether random access seeking operations are enabled
//! - **`seek_mode`**: Balance between seeking speed and accuracy
//! - **`gapless`**: Enable gapless playback for supported formats
//! - **`scan_duration`**: Allow expensive file scanning for duration calculation
//! - **`total_duration`**: Pre-computed duration to avoid file scanning
//!
//! # Format Compatibility
//!
//! Different formats benefit from different configuration approaches:
//! - **MP3**: Benefits from byte length and duration scanning
//! - **FLAC**: Excellent seeking with any configuration
//! - **OGG Vorbis**: Requires seekable flag and benefits from duration scanning
//! - **WAV**: Excellent performance with minimal configuration
//! - **Symphonia formats**: May require format hints for optimal detection

use std::{
    io::{Read, Seek},
    time::Duration,
};

#[cfg(feature = "symphonia")]
use self::read_seek_source::ReadSeekSource;
#[cfg(feature = "symphonia")]
use ::symphonia::core::io::{MediaSource, MediaSourceStream};

use super::*;

/// Seeking modes for audio decoders.
///
/// This enum controls the trade-off between seeking speed and accuracy. Different modes
/// are appropriate for different use cases, and format support varies.
///
/// # Performance Characteristics
///
/// - **Fastest**: Optimized for speed, may sacrifice precision
/// - **Nearest**: Optimized for accuracy, may sacrifice speed
///
/// # Format Support
///
/// Not all formats support both modes equally:
/// - **MP3**: Fastest requires byte length, otherwise falls back to Nearest
/// - **FLAC**: Both modes generally equivalent (always accurate)
/// - **OGG Vorbis**: Fastest uses granule-based seeking, Nearest uses linear
/// - **WAV**: Both modes equivalent (always fast and accurate)
/// - **Symphonia**: Mode support varies by underlying format
///
/// # Use Case Guidelines
///
/// - **User scrubbing**: Use Fastest for responsive UI
/// - **Gapless playback**: Use Nearest for seamless transitions
/// - **Real-time applications**: Use Fastest to minimize latency
/// - **Audio analysis**: Use Nearest for sample-accurate positioning
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SeekMode {
    /// Use the fastest available seeking method with coarse positioning.
    ///
    /// This mode prioritizes speed over precision, using format-specific optimizations
    /// that may result in positioning slightly before or after the requested time,
    /// especially with variable bitrate content.
    ///
    /// # Behavior
    ///
    /// - Uses coarse seeking when available (keyframe-based, byte-position estimation)
    /// - Falls back to nearest seeking when coarse seeking unavailable
    /// - May require additional sample skipping for exact positioning
    /// - Optimal for user interface responsiveness and scrubbing
    ///
    /// # Requirements
    ///
    /// For optimal performance with this mode:
    /// - Set `seekable` to enable backward seeking
    /// - Set `byte_len` for formats that need it (especially MP3)
    /// - Consider format-specific limitations and fallback behavior
    ///
    /// # Performance
    ///
    /// Typically provides O(1) or O(log n) seeking performance depending on format,
    /// making it suitable for real-time applications and responsive user interfaces.
    Fastest,

    /// Use the most accurate seeking method available with precise positioning.
    ///
    /// This mode prioritizes accuracy over speed, seeking to the exact sample requested
    /// whenever possible. It provides sample-accurate positioning for applications
    /// requiring precise timing control.
    ///
    /// # Behavior
    ///
    /// - Uses accurate seeking when available (sample-level positioning)
    /// - Falls back to nearest seeking with refinement when accurate unavailable
    /// - Falls back to coarse seeking only as last resort
    /// - Performs additional sample-level positioning for exact results
    ///
    /// # Requirements
    ///
    /// For optimal accuracy with this mode:
    /// - Set `seekable` to enable full seeking capabilities
    /// - Set `byte_len` for reliable positioning calculations
    /// - Expect potentially slower seeking performance
    ///
    /// # Performance
    ///
    /// May provide O(n) seeking performance for some formats due to linear
    /// sample consumption, but guarantees sample-accurate results.
    ///
    /// # Use Cases
    ///
    /// Ideal for gapless playback, audio analysis, precise editing operations,
    /// and any application where exact positioning is more important than speed.
    #[default]
    Nearest,
}

/// Audio decoder configuration settings.
///
/// This structure contains all configurable parameters that affect decoder behavior,
/// performance, and capabilities. Settings are organized into logical groups for
/// different aspects of decoder operation.
///
/// # Settings Categories
///
/// - **Stream properties**: `byte_len`, `is_seekable`, `total_duration`
/// - **Format detection**: `hint`, `mime_type`
/// - **Seeking behavior**: `seek_mode`, `is_seekable`
/// - **Playback features**: `gapless`, `scan_duration`
///
/// # Decoder Support
///
/// Support for these settings varies by decoder implementation:
/// - Some settings are universal (e.g., `is_seekable`)
/// - Others are format-specific (e.g., `gapless` for supported formats)
/// - Unsupported settings are typically ignored gracefully
#[derive(Clone, Debug)]
pub(super) struct Settings {
    /// The total length of the stream in bytes.
    pub byte_len: Option<u64>,

    /// The seeking mode controlling speed vs. accuracy trade-offs.
    pub seek_mode: SeekMode,

    /// Whether to enable gapless playback by trimming padding frames.
    pub gapless: bool,

    /// Format extension hint for accelerated format detection.
    pub hint: Option<String>,

    /// MIME type hint for container format identification.
    pub mime_type: Option<String>,

    /// Whether the decoder should report seeking capabilities.
    pub is_seekable: bool,

    /// Pre-computed total duration to avoid expensive file scanning.
    pub total_duration: Option<Duration>,

    /// Enable expensive file scanning for accurate duration computation.
    pub scan_duration: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            byte_len: None,
            seek_mode: SeekMode::default(),
            gapless: true,
            hint: None,
            mime_type: None,
            is_seekable: false,
            total_duration: None,
            scan_duration: true,
        }
    }
}

/// Builder for configuring and creating audio decoders.
///
/// This builder provides a fluent API for configuring decoder settings before creation.
/// It follows the builder pattern to enable method chaining and ensures that all
/// necessary configuration is provided before decoder instantiation.
///
/// # Design Philosophy
///
/// The builder is designed to:
/// - **Prevent invalid configurations**: Validates settings at build time
/// - **Optimize performance**: Enables format-specific optimizations
/// - **Simplify common cases**: Provides sensible defaults for most use cases
/// - **Support advanced scenarios**: Allows fine-grained control when needed
///
/// # Configuration Flow
///
/// 1. **Create builder**: `DecoderBuilder::new()`
/// 2. **Set data source**: `.with_data(source)`
/// 3. **Configure options**: `.with_hint()`, `.with_seekable()`, etc.
/// 4. **Build decoder**: `.build()` or `.build_looped()`
///
/// # Error Handling
///
/// The builder defers most validation to build time, allowing for flexible
/// configuration while ensuring that invalid combinations are caught before
/// decoder creation.
///
/// # Examples
///
/// ## Basic File Decoding
///
/// ```no_run
/// use std::fs::File;
/// use rodio::decoder::DecoderBuilder;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let file = File::open("audio.mp3")?;
///     let decoder = DecoderBuilder::new()
///         .with_data(file)
///         .with_hint("mp3")
///         .with_gapless(true)
///         .build()?;
///
///     // Use the decoder...
///     Ok(())
/// }
/// ```
///
/// ## High-Performance Configuration
///
/// ```no_run
/// use std::fs::File;
/// use rodio::{decoder::DecoderBuilder, decoder::builder::SeekMode};
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let file = File::open("audio.flac")?;
///     let len = file.metadata()?.len();
///
///     let decoder = DecoderBuilder::new()
///         .with_data(file)
///         .with_byte_len(len)
///         .with_hint("flac")
///         .with_seekable(true)
///         .with_seek_mode(SeekMode::Fastest)
///         .build()?;
///
///     // Optimized decoder ready for use
///     Ok(())
/// }
/// ```
#[derive(Clone, Debug)]
pub struct DecoderBuilder<R> {
    /// The input data source to decode.
    ///
    /// Holds the audio data stream until decoder creation. Must implement
    /// `Read + Seek + Send + Sync` for compatibility with all decoder types.
    data: Option<R>,

    /// Configuration settings for the decoder.
    ///
    /// Contains all parameters that will be used to configure the decoder
    /// behavior, performance characteristics, and feature enablement.
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
    /// Initializes the builder with sensible defaults suitable for most use cases:
    /// - Gapless playback enabled
    /// - Nearest seeking mode (accuracy over speed)
    /// - No format hints (universal detection)
    /// - Seeking disabled (streaming-friendly)
    /// - Duration scanning disabled
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::fs::File;
    /// use rodio::decoder::DecoderBuilder;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let file = File::open("audio.mp3")?;
    ///     let decoder = DecoderBuilder::new()
    ///         .with_data(file)
    ///         .build()?;
    ///
    ///     // Use the decoder with default settings
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Default Configuration
    ///
    /// The default configuration prioritizes compatibility and quality over performance,
    /// making it suitable for most applications without additional configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the input data source to decode.
    ///
    /// The data source must implement `Read + Seek + Send + Sync` to be compatible
    /// with all decoder implementations. Most standard types like `File`, `Cursor<Vec<u8>>`,
    /// and `BufReader<File>` satisfy these requirements.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::fs::File;
    /// use rodio::decoder::DecoderBuilder;
    ///
    /// let file = File::open("audio.wav").unwrap();
    /// let builder = DecoderBuilder::new().with_data(file);
    /// ```
    ///
    /// # Requirements
    ///
    /// The data source must remain valid for the lifetime of the decoder,
    /// as seeking and reading operations will be performed on it throughout
    /// the decoding process.
    pub fn with_data(mut self, data: R) -> Self {
        self.data = Some(data);
        self
    }

    /// Sets the byte length of the stream.
    ///
    /// Depending on the format this can enable several important optimizations:
    /// - **Seeking operations**: Required for reliable backward seeking
    /// - **Duration calculations**: Essential for formats lacking timing metadata
    /// - **Progress indication**: Enables accurate progress tracking
    /// - **Buffer optimization**: Helps with memory management decisions
    ///
    /// Note that this also sets `is_seekable` to `true`.
    ///
    /// # Format Requirements
    ///
    /// - **MP3**: Required for coarse seeking and duration scanning
    /// - **OGG Vorbis**: Used for duration scanning optimization
    /// - **FLAC/WAV**: Improves seeking performance but not strictly required
    /// - **Symphonia**: May be used for internal optimizations
    ///
    /// # Obtaining Byte Length
    ///
    /// Can be obtained from:
    /// - File metadata: `file.metadata()?.len()`
    /// - Stream seeking: `stream.seek(SeekFrom::End(0))?`
    /// - HTTP Content-Length headers for network streams
    ///
    /// `DecoderBuilder::try_from::<File>()` automatically sets this from file metadata.
    /// Alternatively, you can set it manually from file metadata:
    ///
    /// ```no_run
    /// use std::fs::File;
    /// use rodio::Decoder;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let file = File::open("audio.mp3")?;
    ///     let len = file.metadata()?.len();
    ///     let decoder = Decoder::builder()
    ///         .with_data(file)
    ///         .with_byte_len(len)
    ///         .build()?;
    ///
    ///     // Use the decoder...
    ///     Ok(())
    /// }
    /// ```
    ///
    /// The byte length may also be obtained by seeking to the end of the stream:
    ///
    /// ```ignore
    /// let len = data.seek(std::io::SeekFrom::End(0))?;
    /// ```
    ///
    /// An incorrect byte length can lead to unexpected behavior, including but not limited to
    /// incorrect duration calculations and seeking errors.
    pub fn with_byte_len(mut self, byte_len: u64) -> Self {
        self.settings.byte_len = Some(byte_len);
        self.settings.is_seekable = true;
        self
    }

    /// Sets the seeking mode for the decoder.
    ///
    /// This setting affects seeking behavior across all decoder implementations
    /// that support seeking operations. The actual behavior depends on format
    /// capabilities and available optimizations.
    pub fn with_seek_mode(mut self, seek_mode: SeekMode) -> Self {
        self.settings.seek_mode = seek_mode;
        self
    }

    /// Enables or disables coarse seeking. This is disabled by default.
    ///
    /// This may also need `byte_len` to be set. Coarse seeking is faster but less accurate: it may
    /// seek to a position slightly before or after the requested one, especially when the bitrate
    /// is variable.
    #[deprecated(
        note = "Use `with_seek_mode(SeekMode::Fastest)` instead.",
        since = "0.22.0"
    )]
    pub fn with_coarse_seek(mut self, coarse_seek: bool) -> Self {
        if coarse_seek {
            self.settings.seek_mode = SeekMode::Fastest;
        } else {
            self.settings.seek_mode = SeekMode::Nearest;
        }
        self
    }

    /// Enables or disables gapless playback. This is enabled by default.
    ///
    /// When enabled, removes silence padding added during encoding to achieve
    /// seamless transitions between tracks. This is particularly important for
    /// albums designed for continuous playback.
    ///
    /// # Format Support
    ///
    /// - **MP3**: Removes encoder delay and padding frames
    /// - **AAC**: Removes padding specified in container metadata
    /// - **FLAC**: Generally gapless by nature, minimal effect
    /// - **OGG Vorbis**: Handles sample-accurate boundaries
    ///
    /// # Duration Impact
    ///
    /// Enabling gapless may affect duration calculations as padding frames
    /// will be excluded in the total sample count for some decoders. If you
    /// need consistent duration reporting across decoders, consider disabling this.
    pub fn with_gapless(mut self, gapless: bool) -> Self {
        self.settings.gapless = gapless;
        self
    }

    /// Sets a format hint for the decoder.
    ///
    /// Providing an accurate hint significantly improves decoder initialization
    /// performance by reducing the number of format probes required. Common
    /// values include file extensions without the dot.
    ///
    /// # Common Values
    ///
    /// - Codec hints: "aac", "flac", "mp3", "wav"
    /// - Container hints: "audio/x-matroska", "audio/mp4", "audio/ogg"
    ///
    /// For audio within a container, such as MKV, MP4 or Ogg, use the container hint.
    ///
    /// # Performance Impact
    ///
    /// Without hints, decoders must probe all supported formats sequentially,
    /// which can be slow for large format lists or complex containers.
    pub fn with_hint(mut self, hint: &str) -> Self {
        self.settings.hint = Some(hint.to_string());
        self
    }

    /// Sets a mime type hint for the decoder.
    ///
    /// Provides additional information for format detection, particularly useful
    /// for network streams where file extensions may not be available. This
    /// complements the extension hint for comprehensive format identification.
    ///
    /// # Common Values
    ///
    /// - "audio/mpeg" (MP3)
    /// - "audio/flac" (FLAC)
    /// - "audio/ogg" (OGG Vorbis/Opus)
    /// - "audio/mp4" or "audio/aac" (AAC in MP4)
    /// - "audio/wav" or "audio/vnd.wav" (WAV)
    pub fn with_mime_type(mut self, mime_type: &str) -> Self {
        self.settings.mime_type = Some(mime_type.to_string());
        self
    }

    /// Configure whether the data supports random access seeking. Without this, only forward
    /// seeking may work.
    ///
    /// This setting controls whether the decoder will attempt backward seeking
    /// operations. When disabled, only forward seeking (sample skipping) is
    /// allowed, which is suitable for streaming scenarios.
    ///
    /// # Requirements
    ///
    /// For reliable seeking behavior:
    /// - The underlying stream must support `Seek` trait
    /// - `byte_len` should be set for optimal performance
    /// - Some formats may have additional requirements
    ///
    /// # Automatic Setting
    ///
    /// This is automatically set to `true` when `byte_len` is provided,
    /// as byte length information typically implies seekable streams.
    /// `DecoderBuilder::try_from::<File>()` automatically sets this to `true`.
    ///
    /// # Examples
    /// ```no_run
    /// use std::fs::File;
    /// use rodio::Decoder;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let file = File::open("audio.mp3")?;
    ///     let len = file.metadata()?.len();
    ///
    ///     // Recommended: Set both byte_len and seekable
    ///     let decoder = Decoder::builder()
    ///         .with_data(file)
    ///         .with_byte_len(len)
    ///         .with_seekable(true)
    ///         .build()?;
    ///
    ///     // Use the decoder...
    ///     Ok(())
    /// }
    /// ```
    pub fn with_seekable(mut self, is_seekable: bool) -> Self {
        self.settings.is_seekable = is_seekable;
        self
    }

    /// Provides a pre-computed total duration to avoid file scanning.
    ///
    /// When provided, decoders will use this value instead of performing
    /// potentially expensive duration calculation operations. This is
    /// particularly useful when duration is known from external metadata.
    ///
    /// # Use Cases
    ///
    /// - Database-stored track durations
    /// - Previously calculated durations
    /// - External metadata sources (ID3, database, etc.)
    /// - Avoiding redundant file scanning in batch operations
    ///
    /// # Priority
    ///
    /// This setting takes precedence over `scan_duration` when both are set.
    ///
    /// This affects decoder implementations that may scan for duration:
    /// - **MP3**: May scan if metadata doesn't contain duration
    /// - **Vorbis/OGG**: Scans to determine total duration
    /// - **Symphonia**: Not controlled by this setting; may scan if byte_len is set
    ///
    /// # Examples
    /// ```no_run
    /// use std::time::Duration;
    /// use std::fs::File;
    /// use rodio::Decoder;
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let known_duration = Duration::from_secs(180);
    ///
    /// let decoder = Decoder::builder()
    ///     .with_data(file)
    ///     .with_total_duration(known_duration)  // Skip any scanning
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn with_total_duration(mut self, duration: Duration) -> Self {
        self.settings.total_duration = Some(duration);
        self
    }

    /// Enable file scanning for duration computation.
    ///
    /// When enabled, allows decoders to perform comprehensive file analysis
    /// to determine accurate duration information. This can be slow for large
    /// files but provides the most accurate duration data.
    ///
    /// # Prerequisites
    ///
    /// This setting only takes effect when:
    /// - `is_seekable` is `true`
    /// - `byte_len` is set
    /// - The decoder supports duration scanning
    ///
    /// # Format Behavior
    ///
    /// - **MP3**: Scans for XING/VBRI headers, then frame-by-frame if needed
    /// - **OGG Vorbis**: Uses binary search to find last granule position
    /// - **FLAC**: Duration available in metadata, no scanning needed
    /// - **WAV**: Duration available in header, no scanning needed
    ///
    /// # Performance Impact
    ///
    /// Scanning time varies significantly:
    /// - Small files (< 10MB): Usually very fast
    /// - Large files (> 100MB): Can take several seconds
    /// - Variable bitrate files: May require more extensive scanning
    ///
    /// This affects specific decoder implementations:
    /// - **MP3**: May scan if metadata doesn't contain duration
    /// - **Vorbis/OGG**: Scans to determine total duration
    /// - **Symphonia**: Not controlled by this setting; may scan if byte_len is set
    ///
    /// File-based decoders (`Decoder::try_from(file)`) automatically enable this and set the
    /// required prerequisites.
    ///
    /// # Examples
    /// ```no_run
    /// use std::fs::File;
    /// use rodio::Decoder;
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let len = file.metadata().unwrap().len();
    ///
    /// # let file = std::fs::File::open("audio.mp3").unwrap();
    /// # let len = file.metadata().unwrap().len();
    /// let decoder = Decoder::builder()
    ///     .with_data(file)
    ///     .with_byte_len(len)        // Required
    ///     .with_seekable(true)       // Already set by with_byte_len()
    ///     .with_scan_duration(true)  // Now effective
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn with_scan_duration(mut self, scan: bool) -> Self {
        self.settings.scan_duration = scan;
        self
    }

    /// Creates the decoder implementation with configured settings.
    ///
    /// This internal method handles the format detection and decoder creation process.
    /// It attempts to create decoders in a specific order, passing the data source
    /// between attempts until a compatible format is found.
    ///
    /// # Error Handling
    ///
    /// Each decoder attempts format detection and returns either:
    /// - `Ok(decoder)`: Format recognized and decoder created
    /// - `Err(data)`: Format not recognized, data returned for next attempt
    ///
    /// # Performance Optimization
    ///
    /// Format hints can significantly improve this process by allowing
    /// the appropriate decoder to be tried first, reducing detection overhead.
    #[allow(unused_variables)]
    fn build_impl(self) -> Result<(DecoderImpl<R>, Settings), DecoderError> {
        let data = self.data.ok_or(DecoderError::UnrecognizedFormat)?;

        #[cfg(feature = "hound")]
        let data = match wav::WavDecoder::new_with_settings(data, &self.settings) {
            Ok(decoder) => return Ok((DecoderImpl::Wav(decoder), self.settings)),
            Err(data) => data,
        };
        #[cfg(feature = "claxon")]
        let data = match flac::FlacDecoder::new_with_settings(data, &self.settings) {
            Ok(decoder) => return Ok((DecoderImpl::Flac(decoder), self.settings)),
            Err(data) => data,
        };

        #[cfg(feature = "lewton")]
        let data = match vorbis::VorbisDecoder::new_with_settings(data, &self.settings) {
            Ok(decoder) => return Ok((DecoderImpl::Vorbis(decoder), self.settings)),
            Err(data) => data,
        };

        #[cfg(feature = "minimp3")]
        let data = match mp3::Mp3Decoder::new_with_settings(data, &self.settings) {
            Ok(decoder) => return Ok((DecoderImpl::Mp3(decoder), self.settings)),
            Err(data) => data,
        };

        #[cfg(feature = "symphonia")]
        {
            let mss = MediaSourceStream::new(
                Box::new(ReadSeekSource::new(data, &self.settings)) as Box<dyn MediaSource>,
                Default::default(),
            );

            symphonia::SymphoniaDecoder::new_with_settings(mss, &self.settings)
                .map(|decoder| (DecoderImpl::Symphonia(decoder, PhantomData), self.settings))
        }

        #[cfg(not(feature = "symphonia"))]
        Err(DecoderError::UnrecognizedFormat)
    }

    /// Creates a new decoder with previously configured settings.
    ///
    /// This method finalizes the builder configuration and attempts to create
    /// an appropriate decoder for the provided data source. Format detection
    /// is performed automatically unless format hints are provided.
    ///
    /// # Error Handling
    ///
    /// Returns `DecoderError::UnrecognizedFormat` if:
    /// - No data source was provided via `with_data()`
    /// - The audio format could not be determined from the data
    /// - No enabled decoder supports the detected format
    /// - The file is corrupted or incomplete
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
    ///     .build()
    ///     .unwrap();
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - Format hints significantly improve build performance
    /// - Large files may take longer due to format detection
    /// - Duration scanning (if enabled) may add additional build time
    pub fn build(self) -> Result<Decoder<R>, DecoderError> {
        let (decoder, _) = self.build_impl()?;
        Ok(Decoder(decoder))
    }

    /// Creates a new looped decoder with previously configured settings.
    ///
    /// This method creates a decoder that automatically restarts from the beginning
    /// when it reaches the end of the audio stream, providing seamless looping
    /// functionality for background music and ambient audio.
    ///
    /// # Looping Behavior
    ///
    /// The looped decoder:
    /// - Automatically resets to the beginning when the stream ends
    /// - Preserves all configured settings across loop iterations
    /// - Maintains consistent audio quality and timing
    /// - Supports seeking operations within the current loop
    ///
    /// # Error Handling
    ///
    /// Returns the same errors as `build()`:
    /// - `DecoderError::UnrecognizedFormat` for format detection failures
    /// - Other decoder-specific errors during initialization
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::fs::File;
    /// use rodio::decoder::DecoderBuilder;
    ///
    /// let file = File::open("background_music.ogg").unwrap();
    /// let looped_decoder = DecoderBuilder::new()
    ///     .with_data(file)
    ///     .with_hint("ogg")
    ///     .with_gapless(true)
    ///     .build_looped()
    ///     .unwrap();
    /// ```
    ///
    /// # Performance Considerations
    ///
    /// Looped decoders cache duration information to avoid recalculating it
    /// on each loop iteration, improving performance for repeated playback.
    pub fn build_looped(self) -> Result<LoopedDecoder<R>, DecoderError> {
        let (decoder, settings) = self.build_impl()?;
        Ok(LoopedDecoder {
            inner: Some(decoder),
            settings,
            cached_duration: None,
        })
    }
}
