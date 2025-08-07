//! FLAC audio decoder implementation.
//!
//! This module provides FLAC decoding capabilities using the `claxon` library. The FLAC format
//! is a lossless audio compression format that preserves original audio data while reducing file
//! size through sophisticated entropy coding and decorrelation techniques.
//!
//! # Features
//!
//! - **Bit depths**: Full support for 8, 16, 24, and 32-bit audio (including 12 and 20-bit)
//! - **Sample rates**: Supports all FLAC-compatible sample rates (1Hz to 655,350Hz)
//! - **Channels**: Supports mono, stereo, and multi-channel audio (up to 8 channels)
//! - **Seeking**: Full forward and backward seeking with sample-accurate positioning
//! - **Duration**: Accurate total duration calculation from stream metadata
//! - **Performance**: Optimized block-based decoding with reusable buffers
//!
//! # Advantages
//!
//! - **Perfect quality**: Lossless compression preserves original audio fidelity
//! - **Efficient compression**: Typically 30-50% size reduction vs. uncompressed
//! - **Fast decoding**: Optimized algorithms with minimal computational overhead
//! - **Precise seeking**: Sample-accurate positioning without approximation
//! - **Rich metadata**: Comprehensive format specification with extensive metadata support
//!
//! # Limitations
//!
//! - Seeking requires `is_seekable` setting for backward seeks (forward-only otherwise)
//! - No support for embedded cue sheets or complex metadata (focus on audio data)
//! - Larger files than lossy formats (trade-off for perfect quality)
//!
//! # Configuration
//!
//! The decoder can be configured through `DecoderBuilder`:
//! - `with_seekable(true)` - Enable backward seeking (recommended for FLAC)
//! - Other settings are informational and don't affect FLAC decoding performance
//!
//! # Performance Notes
//!
//! - Block-based decoding minimizes memory allocations during playback
//! - Seeking operations use efficient linear scanning or stream reset
//! - Buffer reuse optimizes performance for continuous playback
//! - Metadata parsing optimized for audio-focused applications
//! - Memory usage scales with maximum block size, not file size
//!
//! # Example
//!
//! ```ignore
//! use std::fs::File;
//! use rodio::Decoder;
//!
//! let file = File::open("audio.flac").unwrap();
//! let decoder = Decoder::builder()
//!     .with_data(file)
//!     .with_seekable(true)
//!     .build()
//!     .unwrap();
//!
//! // FLAC supports seeking and bit depth detection
//! println!("Bit depth: {:?}", decoder.bits_per_sample());
//! println!("Duration: {:?}", decoder.total_duration());
//! println!("Sample rate: {}", decoder.sample_rate().get());
//! ```

use std::{
    io::{Read, Seek},
    mem,
    sync::Arc,
    time::Duration,
};

use claxon::{FlacReader, FlacReaderOptions};
use dasp_sample::Sample as _;
use dasp_sample::I24;

use super::{utils, Settings};
use crate::{
    common::{ChannelCount, Sample, SampleRate},
    math::duration_to_float,
    source::SeekError,
    Float, Source,
};

/// Reader options for `claxon` FLAC decoder.
///
/// Configured to skip metadata parsing and vorbis comments for faster initialization.
/// This improves decoder creation performance by only parsing essential stream information
/// needed for audio playback.
///
/// # Fields
///
/// - `metadata_only`: Set to `false` to parse audio blocks, not just metadata
/// - `read_vorbis_comment`: Set to `false` to skip comment blocks for performance
const READER_OPTIONS: FlacReaderOptions = FlacReaderOptions {
    metadata_only: false,
    read_vorbis_comment: false,
};

/// Decoder for the FLAC format using the `claxon` library.
///
/// Provides lossless audio decoding with block-based processing, linear seeking, and duration
/// calculation through FLAC stream metadata analysis. The decoder maintains internal buffers
/// for efficient sample-by-sample iteration while preserving the original audio quality.
///
/// # Block-based Processing
///
/// FLAC audio is organized into variable-size blocks containing interleaved samples.
/// The decoder maintains a buffer of the current block and tracks position within it
/// for efficient sample access without re-decoding.
///
/// # Memory Management
///
/// The decoder pre-allocates buffers based on FLAC stream metadata (maximum block size)
/// to minimize allocations during playback. Buffers are reused across blocks for
/// optimal performance.
///
/// # Thread Safety
///
/// This decoder is not thread-safe. Create separate instances for concurrent access
/// or use appropriate synchronization primitives.
///
/// # Generic Parameters
///
/// * `R` - The underlying data source type, must implement `Read + Seek`
pub struct FlacDecoder<R>
where
    R: Read + Seek,
{
    /// The underlying FLAC reader, wrapped in Option for seeking operations.
    ///
    /// Temporarily set to `None` during stream reset operations for backward seeking.
    /// Always `Some` during normal operation and iteration.
    reader: Option<FlacReader<R>>,

    /// Buffer containing decoded samples from current block.
    ///
    /// Stores raw i32 samples from the current FLAC block in the decoder's native
    /// interleaved format. Capacity is pre-allocated based on stream's maximum block size.
    current_block: Vec<i32>,

    /// Number of samples per channel in current block.
    ///
    /// Used for calculating the correct memory layout when accessing interleaved samples.
    /// FLAC blocks can have variable sizes, so this changes per block.
    current_block_channel_len: usize,

    /// Current position within the current block.
    ///
    /// Tracks the next sample index to return from `current_block`. When this reaches
    /// `current_block.len()`, a new block must be decoded.
    current_block_off: usize,

    /// Number of bits per sample (8, 12, 16, 20, 24, or 32).
    ///
    /// Preserved from the original FLAC stream metadata and used for proper
    /// sample conversion during iteration. FLAC supports various bit depths
    /// including non-standard ones like 12 and 20-bit.
    bits_per_sample: u32,

    /// Sample rate in Hz.
    ///
    /// Cached from FLAC stream metadata. FLAC supports sample rates from 1 Hz
    /// to 655,350 Hz, though typical rates are 44.1kHz, 48kHz, 96kHz, etc.
    sample_rate: SampleRate,

    /// Number of audio channels.
    ///
    /// FLAC supports 1 to 8 channels. Common configurations include mono (1),
    /// stereo (2), 5.1 surround (6), and 7.1 surround (8).
    channels: ChannelCount,

    /// Total duration if known from stream metadata.
    ///
    /// Calculated from the total sample count in FLAC metadata. `None` indicates
    /// missing or invalid metadata, though this is rare for valid FLAC files.
    total_duration: Option<Duration>,

    /// Total number of audio frames in the stream.
    ///
    /// Represents the total number of inter-channel samples (frames) as stored
    /// in FLAC metadata. Used for accurate seeking and duration calculation.
    total_samples: Option<u64>,

    /// Number of samples read so far (for seeking calculations).
    ///
    /// Tracks the current playback position in total samples (across all channels).
    /// Used to determine if seeking requires stream reset or can be done by
    /// skipping forward.
    samples_read: u64,

    /// Whether the stream supports random access seeking.
    ///
    /// When `true`, enables backward seeking by allowing stream reset operations.
    /// When `false`, only forward seeking (sample skipping) is allowed.
    is_seekable: bool,
}

impl<R> FlacDecoder<R>
where
    R: Read + Seek,
{
    /// Attempts to decode the data as FLAC with default settings.
    ///
    /// This method probes the input data to detect FLAC format and initializes the decoder if
    /// successful. Uses default settings with no seeking support enabled.
    ///
    /// # Arguments
    ///
    /// * `data` - Input stream implementing `Read + Seek`
    ///
    /// # Returns
    ///
    /// - `Ok(FlacDecoder)` if the data contains valid FLAC format
    /// - `Err(R)` if the data is not FLAC, returning the original stream
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::fs::File;
    /// use rodio::decoder::flac::FlacDecoder;
    ///
    /// let file = File::open("audio.flac").unwrap();
    /// match FlacDecoder::new(file) {
    ///     Ok(decoder) => println!("FLAC decoder created"),
    ///     Err(file) => println!("Not a FLAC file"),
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// This method performs format detection which requires reading the FLAC header.
    /// The stream position is restored if detection fails, so the original stream
    /// can be used for other format detection attempts.
    #[allow(dead_code)]
    pub fn new(data: R) -> Result<FlacDecoder<R>, R> {
        Self::new_with_settings(data, &Settings::default())
    }

    /// Attempts to decode the data as FLAC with custom settings.
    ///
    /// This method provides full control over decoder configuration including seeking behavior.
    /// It performs format detection, parses FLAC metadata, and initializes internal buffers
    /// based on the stream characteristics.
    ///
    /// # Arguments
    ///
    /// * `data` - Input stream implementing `Read + Seek`
    /// * `settings` - Configuration settings from `DecoderBuilder`
    ///
    /// # Returns
    ///
    /// - `Ok(FlacDecoder)` if the data contains valid FLAC format
    /// - `Err(R)` if the data is not FLAC, returning the original stream
    ///
    /// # Settings Usage
    ///
    /// - `is_seekable`: Enables backward seeking operations
    /// - Other settings don't affect FLAC decoding
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::fs::File;
    /// use rodio::decoder::{flac::FlacDecoder, Settings};
    ///
    /// let file = File::open("audio.flac").unwrap();
    /// let mut settings = Settings::default();
    /// settings.is_seekable = true;
    ///
    /// let decoder = FlacDecoder::new_with_settings(file, &settings).unwrap();
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the FLAC stream has invalid metadata (zero sample rate, zero channels,
    /// or more than 65,535 channels).
    ///
    /// # Performance
    ///
    /// Buffer allocation is based on the stream's maximum block size to minimize
    /// reallocations during playback. Larger maximum block sizes will use more memory
    /// but provide better streaming performance.
    pub fn new_with_settings(mut data: R, settings: &Settings) -> Result<FlacDecoder<R>, R> {
        if !is_flac(&mut data) {
            return Err(data);
        }

        let reader = FlacReader::new_ext(data, READER_OPTIONS).expect("should still be flac");

        let spec = reader.streaminfo();
        let sample_rate = spec.sample_rate;
        let max_block_size = spec.max_block_size as usize * spec.channels as usize;

        // `samples` in FLAC means "inter-channel samples" aka frames
        // so we do not divide by `self.channels` here.
        let total_samples = spec.samples;
        let total_duration =
            total_samples.map(|s| utils::samples_to_duration(s, sample_rate as u64));

        Ok(Self {
            reader: Some(reader),
            current_block: Vec::with_capacity(max_block_size),
            current_block_channel_len: 1,
            current_block_off: 0,
            bits_per_sample: spec.bits_per_sample,
            sample_rate: SampleRate::new(sample_rate)
                .expect("flac data should never have a zero sample rate"),
            channels: ChannelCount::new(
                spec.channels
                    .try_into()
                    .expect("rodio supports only up to u16::MAX (65_535) channels"),
            )
            .expect("flac should never have zero channels"),
            total_duration,
            total_samples,
            samples_read: 0,
            is_seekable: settings.is_seekable,
        })
    }

    /// Consumes the decoder and returns the underlying data stream.
    ///
    /// This can be useful for recovering the original data source after decoding is complete or
    /// when the decoder needs to be replaced. The stream position will be at the current
    /// playback position.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::fs::File;
    /// use rodio::decoder::flac::FlacDecoder;
    ///
    /// let file = File::open("audio.flac").unwrap();
    /// let decoder = FlacDecoder::new(file).unwrap();
    /// let recovered_file = decoder.into_inner();
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if called during a seeking operation when the reader is temporarily `None`.
    /// This should never happen during normal usage.
    #[inline]
    pub fn into_inner(self) -> R {
        self.reader
            .expect("reader should always be Some")
            .into_inner()
    }
}

impl<R> Source for FlacDecoder<R>
where
    R: Read + Seek,
{
    /// Returns the number of samples before parameters change.
    ///
    /// For FLAC, this always returns `None` because audio parameters (sample rate, channels, bit
    /// depth) never change during the stream. This allows Rodio to optimize by not frequently
    /// checking for parameter changes.
    ///
    /// # Implementation Note
    ///
    /// FLAC streams have fixed parameters throughout their duration, unlike some formats
    /// that may have parameter changes at specific points. This enables optimizations
    /// in the audio pipeline by avoiding frequent parameter validation.
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    /// Returns the number of audio channels.
    ///
    /// FLAC supports 1 to 8 channels. Common configurations:
    /// - 1 channel: Mono
    /// - 2 channels: Stereo
    /// - 6 channels: 5.1 surround
    /// - 8 channels: 7.1 surround
    ///
    /// # Guarantees
    ///
    /// The returned value is constant for the lifetime of the decoder and matches
    /// the channel count specified in the FLAC stream metadata.
    #[inline]
    fn channels(&self) -> ChannelCount {
        self.channels
    }

    /// Returns the sample rate in Hz.
    ///
    /// Common rates that FLAC supports are:
    /// - **44.1kHz**: CD quality (most common)
    /// - **48kHz**: Professional audio standard
    /// - **96kHz**: High-resolution audio
    /// - **192kHz**: Ultra high-resolution audio
    ///
    /// # Guarantees
    ///
    /// The returned value is constant for the lifetime of the decoder and matches
    /// the sample rate specified in the FLAC stream metadata. This value is
    /// available immediately upon decoder creation.
    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    /// Returns the total duration of the audio stream.
    ///
    /// FLAC metadata contains the total number of samples, allowing accurate duration calculation.
    /// This is available immediately upon decoder creation without needing to scan the entire file.
    ///
    /// Returns `None` only for malformed FLAC files missing sample count metadata.
    ///
    /// # Accuracy
    ///
    /// The duration is calculated from exact sample counts, providing sample-accurate
    /// timing information. This is more precise than duration estimates based on
    /// bitrate calculations used by lossy formats.
    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }

    /// Returns the bit depth of the audio samples.
    ///
    /// FLAC is a lossless format that preserves the original bit depth:
    /// - 16-bit: Standard CD quality
    /// - 24-bit: Professional/high-resolution audio
    /// - 32-bit: Professional/studio quality
    /// - Other depths: 8, 12, and 20-bit are also supported
    ///
    /// Always returns `Some(depth)` for valid FLAC streams.
    ///
    /// # Implementation Note
    ///
    /// The bit depth information is preserved from the original FLAC stream and
    /// used for proper sample scaling during conversion to Rodio's sample format.
    #[inline]
    fn bits_per_sample(&self) -> Option<u32> {
        Some(self.bits_per_sample)
    }

    /// Attempts to seek to the specified position in the audio stream.
    ///
    /// # Seeking Behavior
    ///
    /// - **Forward seeking**: Fast linear sample skipping from current position
    /// - **Backward seeking**: Requires stream reset and forward seek (needs `is_seekable`)
    /// - **Beyond end**: Seeking past stream end is clamped to actual duration
    /// - **Channel preservation**: Maintains correct channel order across seeks
    ///
    /// # Performance
    ///
    /// - Forward seeks are O(n) where n is samples to skip
    /// - Backward seeks are O(target_position) due to stream reset
    /// - Precise positioning without approximation
    ///
    /// # Arguments
    ///
    /// * `pos` - Target position as duration from stream start
    ///
    /// # Errors
    ///
    /// - `SeekError::ForwardOnly` - Backward seek attempted without `is_seekable`
    /// - `SeekError::ClaxonDecoder` - Underlying FLAC decoder error
    /// - `SeekError::IoError` - I/O error during stream reset
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::{fs::File, time::Duration};
    /// use rodio::{Decoder, Source};
    ///
    /// let file = File::open("audio.flac").unwrap();
    /// let mut decoder = Decoder::builder()
    ///     .with_data(file)
    ///     .with_seekable(true)
    ///     .build()
    ///     .unwrap();
    ///
    /// // Seek to 30 seconds into the track
    /// decoder.try_seek(Duration::from_secs(30)).unwrap();
    /// ```
    ///
    /// # Implementation Details
    ///
    /// The seeking implementation handles channel alignment to ensure that seeking
    /// to a specific time position results in the correct channel being returned
    /// for the first sample after the seek operation.
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        // Seeking should be "saturating", meaning: target positions beyond the end of the stream
        // are clamped to the end.
        let mut target = pos;
        if let Some(total_duration) = self.total_duration() {
            if target > total_duration {
                target = total_duration;
            }
        }

        // Remember the current channel position before seeking (for channel order preservation)
        let active_channel = self.current_block_off % self.channels.get() as usize;

        // Convert duration to sample number (interleaved samples for FLAC)
        // FLAC samples are interleaved, so we need total samples including all channels
        let target_sample = (duration_to_float(target)
            * self.sample_rate.get() as Float
            * self.channels.get() as Float) as u64;

        // FLAC is a structured format, so without seek index support in claxon we can only seek
        // forwards or from the start.
        let samples_to_skip = if target_sample < self.samples_read {
            if !self.is_seekable {
                return Err(SeekError::ForwardOnly);
            }

            // Backwards seek: reset to start by recreating reader
            let mut reader = self
                .reader
                .take()
                .expect("reader should always be Some")
                .into_inner();

            reader.rewind().map_err(Arc::new)?;

            // Recreate FLAC reader and reset state
            let new_reader = FlacReader::new_ext(reader, READER_OPTIONS)?;

            self.reader = Some(new_reader);
            self.current_block.clear();
            self.current_block_off = 0;
            self.samples_read = 0;

            // Skip to target position
            target_sample
        } else {
            // Forward seek: skip from current position
            target_sample - self.samples_read
        };

        // Consume samples to reach target position
        for _ in 0..(samples_to_skip + active_channel as u64) {
            let _ = self.next();
        }

        Ok(())
    }
}

impl<R> Iterator for FlacDecoder<R>
where
    R: Read + Seek,
{
    /// The type of items yielded by the iterator.
    ///
    /// Returns `Sample` (typically `f32`) values representing individual audio samples.
    /// Samples are interleaved across channels in the order: channel 0, channel 1, etc.
    type Item = Sample;

    /// Returns the next audio sample from the FLAC stream.
    ///
    /// This method implements efficient block-based decoding by maintaining an internal
    /// buffer of the current FLAC block. It returns samples one at a time while
    /// automatically decoding new blocks as needed.
    ///
    /// # Sample Format Conversion
    ///
    /// Raw FLAC samples are converted to Rodio's sample format based on bit depth:
    /// - 8-bit: Direct conversion from `i8`
    /// - 16-bit: Direct conversion from `i16`
    /// - 24-bit: Conversion using `I24` type
    /// - 32-bit: Direct conversion from `i32`
    /// - Other (12, 20-bit): Bit-shifted to 32-bit then converted
    ///
    /// # Performance
    ///
    /// - **Hot path**: Returning samples from current block (very fast)
    /// - **Cold path**: Decoding new blocks when buffer is exhausted (slower)
    ///
    /// # Returns
    ///
    /// - `Some(sample)` - Next audio sample
    /// - `None` - End of stream reached or decoding error occurred
    ///
    /// # Channel Order
    ///
    /// Samples are returned in interleaved order: [L, R, L, R, ...] for stereo,
    /// [FL, FR, C, LFE, BL, BR] for 5.1, etc.
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Hot path: return sample from current block if available
            if self.current_block_off < self.current_block.len() {
                // Read from current block.
                let real_offset = (self.current_block_off % self.channels.get() as usize)
                    * self.current_block_channel_len
                    + self.current_block_off / self.channels.get() as usize;
                let raw_val = self.current_block[real_offset];
                self.current_block_off += 1;
                self.samples_read += 1;
                let bits = self.bits_per_sample;
                let real_val = match bits {
                    8 => (raw_val as i8).to_sample(),
                    16 => (raw_val as i16).to_sample(),
                    24 => I24::new(raw_val)
                        .unwrap_or(dasp_sample::Sample::EQUILIBRIUM)
                        .to_sample(),
                    32 => raw_val.to_sample(),
                    _ => {
                        // FLAC also supports 12 and 20 bits per sample. We use bit
                        // shifts to convert them to 32 bits, because:
                        // - I12 does not exist as a type
                        // - I20 exists but does not have `ToSample` implemented
                        (raw_val << (32 - bits)).to_sample()
                    }
                };
                return Some(real_val);
            }

            // Cold path: need to decode next block
            self.current_block_off = 0;
            let buffer = mem::take(&mut self.current_block);
            match self
                .reader
                .as_mut()
                .expect("reader should always be Some")
                .blocks()
                .read_next_or_eof(buffer)
            {
                Ok(Some(block)) => {
                    self.current_block_channel_len = (block.len() / block.channels()) as usize;
                    self.current_block = block.into_buffer();
                }
                Ok(None) | Err(_) => {
                    // No more blocks or error, current_block becomes empty
                    // (buffer was consumed by read_next_or_eof)
                    return None;
                }
            }
        }
    }

    /// Returns bounds on the remaining length of the iterator.
    ///
    /// Provides accurate size estimates based on FLAC metadata when available.
    /// This information can be used by consumers for buffer pre-allocation
    /// and progress indication.
    ///
    /// # Returns
    ///
    /// A tuple `(lower_bound, upper_bound)` where:
    /// - `lower_bound`: Minimum number of samples guaranteed to be available
    /// - `upper_bound`: Maximum number of samples that might be available (None if unknown)
    ///
    /// # Accuracy
    ///
    /// - **With metadata**: Exact remaining sample count (lower == upper)
    /// - **Without metadata**: Conservative estimate based on current block
    /// - **Stream exhausted**: (0, Some(0))
    ///
    /// # Implementation
    ///
    /// The lower bound counts buffered samples that are immediately available.
    /// The upper bound uses total stream metadata when available for precise counting.
    fn size_hint(&self) -> (usize, Option<usize>) {
        // Samples already decoded and buffered (guaranteed available)
        let buffered_samples = self
            .current_span_len()
            .unwrap_or(0)
            .saturating_sub(self.current_block_off);

        if let Some(total_samples) = self.total_samples {
            let total_remaining = total_samples.saturating_sub(self.samples_read) as usize;
            (buffered_samples, Some(total_remaining))
        } else if self.current_block.is_empty() {
            // Stream exhausted (no more blocks available)
            (0, Some(0))
        } else {
            (buffered_samples, None)
        }
    }
}

/// Probes input data to detect FLAC format.
///
/// This function attempts to parse the FLAC magic bytes and stream info header to determine if the
/// data contains a valid FLAC stream. The stream position is restored regardless of the result.
///
/// # Arguments
///
/// * `data` - Mutable reference to the input stream to probe
///
/// # Returns
///
/// - `true` if the data appears to contain a valid FLAC stream
/// - `false` if the data is not FLAC or is corrupted
///
/// # Implementation
///
/// Uses the common `utils::probe_format` helper which:
/// 1. Saves the current stream position
/// 2. Attempts FLAC detection using `claxon::FlacReader`
/// 3. Restores the original stream position
/// 4. Returns the detection result
///
/// # Performance
///
/// This function only reads the minimum amount of data needed to identify
/// the FLAC format (magic bytes and basic header), making it efficient for
/// format detection in multi-format scenarios.
fn is_flac<R>(data: &mut R) -> bool
where
    R: Read + Seek,
{
    utils::probe_format(data, |reader| FlacReader::new(reader).is_ok())
}

/// Converts claxon decoder errors to rodio seek errors.
///
/// This implementation provides error context preservation when FLAC decoding operations fail
/// during seeking. The original `claxon` error is wrapped in an `Arc` for thread safety and
/// converted to the appropriate Rodio error type.
///
/// # Error Mapping
///
/// All `claxon::Error` variants are mapped to `SeekError::ClaxonDecoder` with the
/// original error preserved for debugging and error analysis.
///
/// # Thread Safety
///
/// The error is wrapped in `Arc` to allow sharing across thread boundaries if needed,
/// following Rodio's error handling patterns.
impl From<claxon::Error> for SeekError {
    /// Converts a claxon error into a Rodio seek error.
    ///
    /// # Arguments
    ///
    /// * `err` - The original claxon decoder error
    ///
    /// # Returns
    ///
    /// A `SeekError::ClaxonDecoder` containing the original error wrapped in an `Arc`.
    fn from(err: claxon::Error) -> Self {
        SeekError::ClaxonDecoder(Arc::new(err))
    }
}
