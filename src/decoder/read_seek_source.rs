//! Read + Seek adapter for Symphonia MediaSource integration.
//!
//! This module provides a bridge between standard Rust I/O types and Symphonia's
//! MediaSource trait, enabling seamless integration of file handles, cursors, and
//! other I/O sources with Symphonia's audio decoding framework.
//!
//! # Purpose
//!
//! Symphonia requires audio sources to implement its `MediaSource` trait, which
//! provides metadata about stream characteristics like seekability and byte length.
//! This adapter wraps standard Rust I/O types to provide this interface.
//!
//! # Architecture
//!
//! The adapter acts as a thin wrapper that:
//! - Delegates I/O operations to the wrapped type
//! - Provides stream metadata from decoder settings
//! - Maintains compatibility with Symphonia's requirements
//! - Preserves performance characteristics of the underlying source
//!
//! # Performance
//!
//! The wrapper has minimal overhead:
//! - Zero-cost delegation for read/seek operations
//! - Inline functions for optimal performance
//! - No additional buffering or copying
//! - Metadata cached from initial configuration

use std::io::{Read, Result, Seek, SeekFrom};

use symphonia::core::io::MediaSource;

use super::Settings;

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
/// # Thread Safety
///
/// This wrapper requires `Send + Sync` bounds on the wrapped type to ensure
/// thread safety for Symphonia's internal operations. Most standard I/O types
/// satisfy these requirements.
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
    /// # Examples
    ///
    /// ```ignore
    /// use std::fs::File;
    /// use rodio::decoder::{Settings, read_seek_source::ReadSeekSource};
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let mut settings = Settings::default();
    /// settings.byte_len = Some(1024000);
    /// settings.is_seekable = true;
    ///
    /// let source = ReadSeekSource::new(file, &settings);
    /// ```
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
    /// When `false`, Symphonia will:
    /// - Avoid backward seeking operations
    /// - Use streaming-optimized algorithms
    /// - May provide degraded seeking functionality
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
    /// - **Progress tracking**: Determining playback progress percentage
    /// - **Format detection**: Some formats benefit from knowing stream length
    /// - **Buffer optimization**: Memory allocation decisions
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
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
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
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        self.inner.seek(pos)
    }
}
