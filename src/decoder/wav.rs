//! WAV audio decoder implementation.
//!
//! This module provides WAV decoding capabilities using the `hound` library. WAV is an
//! uncompressed audio format that stores PCM audio data with various bit depths and sample rates.
//! The format uses the RIFF (Resource Interchange File Format) container with WAVE chunks
//! containing audio metadata and raw PCM samples.
//!
//! # Features
//!
//! - **Bit depths**: Full support for 8, 16, 24, and 32-bit integer PCM plus 32-bit float
//! - **Sample rates**: Supports all WAV-compatible sample rates (typically 8kHz to 192kHz)
//! - **Channels**: Supports mono, stereo, and multi-channel audio up to system limits
//! - **Seeking**: Fast random access seeking with sample-accurate positioning
//! - **Duration**: Instant duration calculation from WAV header information
//! - **Performance**: Direct sample access with optimized buffering and no decompression
//!
//! # Advantages
//!
//! - **Zero latency**: No compression/decompression overhead
//! - **Perfect quality**: Lossless storage preserves original audio fidelity
//! - **Fast seeking**: Direct sample access without stream scanning
//! - **Simple format**: Reliable parsing with well-defined structure
//! - **Universal support**: Widely supported across all audio applications
//!
//! # Limitations
//!
//! - No support for compressed WAV variants (ADPCM, μ-law, A-law, etc.)
//! - Forward-only seeking without `is_seekable` setting
//! - Limited to 32-bit sample indexing (4.2 billion samples max)
//! - Large file sizes due to uncompressed storage
//! - No embedded metadata beyond basic audio parameters
//!
//! # Configuration
//!
//! The decoder can be configured through `DecoderBuilder`:
//! - `with_seekable(true)` - Enable random access seeking (recommended for WAV)
//! - Other settings are informational and don't affect WAV decoding performance
//!
//! # Performance Notes
//!
//! - Header parsing is extremely fast (single read operation)
//! - No buffering overhead - samples read directly from file
//! - Seeking operations have O(1) complexity via direct file positioning
//! - Memory usage scales only with iterator state, not file size
//!
//! # Example
//!
//! ```ignore
//! use std::fs::File;
//! use rodio::Decoder;
//!
//! let file = File::open("audio.wav").unwrap();
//! let decoder = Decoder::builder()
//!     .with_data(file)
//!     .with_seekable(true)
//!     .build()
//!     .unwrap();
//!
//! // WAV supports seeking and bit depth detection
//! println!("Bit depth: {:?}", decoder.bits_per_sample());
//! println!("Duration: {:?}", decoder.total_duration());
//! println!("Sample rate: {}", decoder.sample_rate().get());
//! ```

use std::{
    io::{Read, Seek},
    sync::Arc,
    time::Duration,
};

use dasp_sample::Sample as _;
use dasp_sample::I24;
use hound::{SampleFormat, WavReader};

use super::utils;
use crate::{
    decoder::builder::Settings, source::SeekError, BitDepth, ChannelCount, Sample, SampleRate,
    Source,
};

/// Decoder for the WAV format using the `hound` library.
///
/// This decoder provides uncompressed PCM audio decoding with fast seeking and instant
/// duration calculation. WAV files contain header information that enables immediate
/// access to format parameters and file length without requiring stream analysis.
///
/// # RIFF/WAVE Structure
///
/// WAV files use the RIFF container format with WAVE chunks:
/// - **RIFF header**: File identification and size information
/// - **fmt chunk**: Audio format parameters (sample rate, channels, bit depth)
/// - **data chunk**: Raw PCM audio samples in little-endian format
/// - **Optional chunks**: May contain additional metadata (ignored by decoder)
///
/// # Sample Format Support
///
/// The decoder handles various PCM formats seamlessly:
/// - **8-bit integer**: Unsigned values (0-255) converted to signed range
/// - **16-bit integer**: Standard CD-quality signed samples
/// - **24-bit integer**: High-resolution audio packed in 32-bit containers
/// - **32-bit integer**: Maximum precision integer samples
/// - **32-bit float**: IEEE 754 floating-point samples (-1.0 to +1.0 range)
///
/// # Performance Characteristics
///
/// - **Header-based duration**: No file scanning required (instant calculation)
/// - **Direct random access**: O(1) seeking via hound's seek functionality
/// - **Optimized sample conversion**: Efficient bit depth handling
/// - **Minimal memory overhead**: Iterator-based sample access
/// - **Zero decompression**: Direct PCM data access
///
/// # Seeking Behavior
///
/// - **Random access seeking**: Direct positioning via hound's seek API
/// - **Forward seeking**: Linear sample skipping when not seekable
/// - **Beyond end**: Seeking past file end is clamped to actual length
/// - **Channel preservation**: Maintains correct channel order across seeks
/// - **Sample accuracy**: Precise positioning without approximation
///
/// # Generic Parameters
///
/// * `R` - The underlying data source type, must implement `Read + Seek`
pub struct WavDecoder<R>
where
    R: Read + Seek,
{
    /// Iterator over audio samples with position tracking.
    ///
    /// Wraps the hound WavReader and provides sample-by-sample iteration
    /// with position tracking for seeking operations and size hints.
    reader: SamplesIterator<R>,

    /// Total duration calculated from WAV header.
    ///
    /// Computed from sample count and sample rate information in the WAV header.
    /// Always available immediately upon decoder creation without file scanning.
    total_duration: Duration,

    /// Sample rate in Hz from WAV header.
    ///
    /// Fixed for the entire WAV file. Common rates include 44.1kHz (CD),
    /// 48kHz (professional), 96kHz/192kHz (high-resolution).
    sample_rate: SampleRate,

    /// Number of audio channels from WAV header.
    ///
    /// Fixed for the entire WAV file. Common configurations include
    /// mono (1), stereo (2), and various surround sound formats.
    channels: ChannelCount,

    /// Bit depth of the audio samples (cached from reader).
    ///
    /// Cached from the WAV header without repeatedly constructing it.
    bits_per_sample: BitDepth,

    /// Whether random access seeking is enabled.
    ///
    /// When `true`, enables backward seeking using hound's direct seek functionality.
    /// When `false`, only forward seeking (sample skipping) is allowed.
    is_seekable: bool,
}

impl<R> WavDecoder<R>
where
    R: Read + Seek,
{
    /// Attempts to decode the data as WAV with default settings.
    ///
    /// This method probes the input data to detect WAV format and initializes the decoder if
    /// successful. Uses default settings with no seeking support enabled, though WAV seeking
    /// is highly recommended due to its efficiency.
    ///
    /// # Arguments
    ///
    /// * `data` - Input stream implementing `Read + Seek`
    ///
    /// # Returns
    ///
    /// - `Ok(WavDecoder)` if the data contains valid WAV format
    /// - `Err(R)` if the data is not WAV, returning the original stream
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::fs::File;
    /// use rodio::decoder::wav::WavDecoder;
    ///
    /// let file = File::open("audio.wav").unwrap();
    /// match WavDecoder::new(file) {
    ///     Ok(decoder) => println!("WAV decoder created"),
    ///     Err(file) => println!("Not a WAV file"),
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// This method performs format detection which requires parsing the RIFF/WAVE headers.
    /// WAV header parsing is very fast as it only requires reading the first few bytes.
    /// The stream position is restored if detection fails.
    #[allow(dead_code)]
    pub fn new(data: R) -> Result<WavDecoder<R>, R> {
        Self::new_with_settings(data, &Settings::default())
    }

    /// Attempts to decode the data as WAV with custom settings.
    ///
    /// This method provides control over decoder configuration, particularly seeking behavior.
    /// It performs format detection, parses WAV headers, and initializes the decoder with
    /// immediate access to all stream characteristics.
    ///
    /// # Arguments
    ///
    /// * `data` - Input stream implementing `Read + Seek`
    /// * `settings` - Configuration settings from `DecoderBuilder`
    ///
    /// # Returns
    ///
    /// - `Ok(WavDecoder)` if the data contains valid WAV format
    /// - `Err(R)` if the data is not WAV, returning the original stream
    ///
    /// # Settings Usage
    ///
    /// - `is_seekable`: Enables random access seeking (highly recommended for WAV)
    /// - Other settings are informational and don't affect WAV decoding
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::fs::File;
    /// use rodio::decoder::{wav::WavDecoder, Settings};
    ///
    /// let file = File::open("audio.wav").unwrap();
    /// let mut settings = Settings::default();
    /// settings.is_seekable = true;
    ///
    /// let decoder = WavDecoder::new_with_settings(file, &settings).unwrap();
    /// ```
    ///
    /// # Performance
    ///
    /// WAV initialization is extremely fast as all information is available in the header:
    /// - Format parameters: Immediate access from fmt chunk
    /// - Duration calculation: Direct from sample count and rate
    /// - No scanning required: Unlike compressed formats
    ///
    /// # Panics
    ///
    /// Panics if the WAV file has invalid characteristics (zero sample rate or zero channels).
    /// This should never happen with valid WAV data that passes format detection.
    pub fn new_with_settings(mut data: R, settings: &Settings) -> Result<WavDecoder<R>, R> {
        if !is_wave(&mut data) {
            return Err(data);
        }

        let reader = WavReader::new(data).expect("should still be wav");
        let spec = reader.spec();
        let len = reader.len() as u64;
        let total_samples = reader.len();
        let reader = SamplesIterator {
            reader,
            samples_read: 0,
            total_samples,
        };

        let sample_rate = spec.sample_rate;
        let channels = spec.channels;

        // len is number of samples, not bytes, so use samples_to_duration
        // Note: hound's len() returns total samples across all channels
        let samples_per_channel = len / (channels as u64);
        let total_duration = utils::samples_to_duration(samples_per_channel, sample_rate as u64);

        Ok(Self {
            reader,
            total_duration,
            sample_rate: SampleRate::new(sample_rate)
                .expect("wav should have a sample rate higher then zero"),
            channels: ChannelCount::new(channels).expect("wav should have a least one channel"),
            is_seekable: settings.is_seekable,
            bits_per_sample: BitDepth::new(spec.bits_per_sample.into())
                .expect("wav should have a bit depth higher then zero"),
        })
    }

    /// Consumes the decoder and returns the underlying data stream.
    ///
    /// This can be useful for recovering the original data source after decoding is complete
    /// or when the decoder needs to be replaced. The stream position will be at the current
    /// playback position within the WAV data chunk.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::fs::File;
    /// use rodio::decoder::wav::WavDecoder;
    ///
    /// let file = File::open("audio.wav").unwrap();
    /// let decoder = WavDecoder::new(file).unwrap();
    /// let recovered_file = decoder.into_inner();
    /// ```
    ///
    /// # Stream Position
    ///
    /// The returned stream will be positioned at the current sample location within
    /// the WAV file's data chunk, which may be useful for manual data processing
    /// or format conversion operations.
    #[inline]
    pub fn into_inner(self) -> R {
        self.reader.reader.into_inner()
    }
}

/// Internal iterator for WAV sample reading with position tracking.
///
/// This struct wraps the hound `WavReader` and tracks the current position
/// for seeking operations and size hints. It handles the complexity of different
/// sample formats while providing a unified interface to the decoder.
///
/// # Position Tracking
///
/// Maintains accurate sample count for:
/// - Seeking calculations and channel alignment
/// - Size hint accuracy for buffer allocation
/// - Progress tracking for long files
///
/// # Sample Format Handling
///
/// Automatically handles conversion from WAV's various sample formats to
/// Rodio's unified sample format, including proper scaling and sign conversion.
struct SamplesIterator<R>
where
    R: Read + Seek,
{
    /// The underlying hound WAV reader.
    ///
    /// Provides access to WAV header information and sample data.
    /// Handles RIFF/WAVE parsing and validates format compliance.
    reader: WavReader<R>,

    /// Number of samples read so far (for seeking calculations).
    ///
    /// Used to track current position for seeking operations and
    /// to calculate remaining samples for size hints. Limited to
    /// u32 range matching WAV format limitations.
    samples_read: u32, // wav header is u32 so this suffices

    /// Total number of samples in the file (cached from reader.len()).
    ///
    /// Cached from the WAV header for efficient size hint calculations
    /// without repeatedly querying the reader.
    total_samples: u32,
}

impl<R> Iterator for SamplesIterator<R>
where
    R: Read + Seek,
{
    /// The type of samples yielded by the iterator.
    ///
    /// Returns `Sample` values representing individual audio samples.
    /// Samples are interleaved across channels in the order: channel 0, channel 1, etc.
    type Item = Sample;

    /// Returns the next audio sample from the WAV stream.
    ///
    /// This method handles conversion from various WAV sample formats to Rodio's
    /// unified sample format. It reads samples directly from the WAV file without
    /// any buffering or decompression overhead.
    ///
    /// # Sample Format Conversion
    ///
    /// The method handles different WAV formats:
    /// - **8-bit integer**: Converts unsigned (0-255) to signed range
    /// - **16-bit integer**: Direct conversion from signed samples
    /// - **24-bit integer**: Extracts from 32-bit container using I24 type
    /// - **32-bit integer**: Direct conversion from signed samples
    /// - **32-bit float**: Direct use of IEEE 754 floating-point values
    /// - **Other integer depths**: Bit-shifting for unofficial formats
    ///
    /// # Error Handling
    ///
    /// - **Unsupported formats**: Logs error and returns None (stream termination)
    /// - **Read errors**: Returns None (end of stream or I/O error)
    /// - **Invalid samples**: Skipped with error logging when possible
    ///
    /// # Performance
    ///
    /// Direct sample access without intermediate buffering provides optimal
    /// performance for WAV files. Sample format conversion is optimized for
    /// each bit depth with minimal computational overhead.
    ///
    /// # Returns
    ///
    /// - `Some(sample)` - Next audio sample from the WAV file
    /// - `None` - End of file reached or unrecoverable error occurred
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.samples_read += 1;
        let spec = self.reader.spec();
        let next_sample: Option<Self::Item> =
            match (spec.sample_format, spec.bits_per_sample as u32) {
                (SampleFormat::Float, bits) => {
                    if bits == 32 {
                        let next_f32: Option<Result<f32, _>> = self.reader.samples().next();
                        next_f32.and_then(|value| value.ok())
                    } else {
                        #[cfg(feature = "tracing")]
                        tracing::error!("Unsupported WAV float bit depth: {}", bits);
                        #[cfg(not(feature = "tracing"))]
                        eprintln!("Unsupported WAV float bit depth: {}", bits);
                        None
                    }
                }

                (SampleFormat::Int, 8) => {
                    let next_i8: Option<Result<i8, _>> = self.reader.samples().next();
                    next_i8.and_then(|value| value.ok().map(|value| value.to_sample()))
                }
                (SampleFormat::Int, 16) => {
                    let next_i16: Option<Result<i16, _>> = self.reader.samples().next();
                    next_i16.and_then(|value| value.ok().map(|value| value.to_sample()))
                }
                (SampleFormat::Int, 24) => {
                    let next_i24_in_i32: Option<Result<i32, _>> = self.reader.samples().next();
                    next_i24_in_i32.and_then(|value| {
                        value.ok().and_then(I24::new).map(|value| value.to_sample())
                    })
                }
                (SampleFormat::Int, 32) => {
                    let next_i32: Option<Result<i32, _>> = self.reader.samples().next();
                    next_i32.and_then(|value| value.ok().map(|value| value.to_sample()))
                }
                (SampleFormat::Int, bits) => {
                    // Unofficial WAV integer bit depth, try to handle it anyway
                    let next_i32: Option<Result<i32, _>> = self.reader.samples().next();
                    if bits <= 32 {
                        next_i32.and_then(|value| {
                            value.ok().map(|value| (value << (32 - bits)).to_sample())
                        })
                    } else {
                        #[cfg(feature = "tracing")]
                        tracing::error!("Unsupported WAV integer bit depth: {}", bits);
                        #[cfg(not(feature = "tracing"))]
                        eprintln!("Unsupported WAV integer bit depth: {}", bits);
                        None
                    }
                }
            };
        next_sample
    }

    /// Returns bounds on the remaining amount of samples.
    ///
    /// For WAV files, this provides exact remaining sample count based on
    /// header information and current position. This enables accurate
    /// buffer pre-allocation and progress indication.
    ///
    /// # Accuracy
    ///
    /// WAV files provide exact sample counts in their headers, making the
    /// upper bound completely accurate. This is more precise than compressed
    /// formats that require estimation or scanning.
    ///
    /// # Implementation
    ///
    /// Uses cached total sample count and current position to calculate
    /// remaining samples with no I/O overhead.
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.total_samples.saturating_sub(self.samples_read) as usize;
        (0, Some(remaining))
    }
}

impl<R> Source for WavDecoder<R>
where
    R: Read + Seek,
{
    /// Returns the number of samples before parameters change.
    ///
    /// For WAV files, this always returns `None` because audio parameters
    /// (sample rate, channels, bit depth) never change during the stream.
    /// WAV files have fixed parameters throughout their duration, enabling
    /// optimizations in the audio pipeline.
    ///
    /// # Implementation Note
    ///
    /// WAV files have a single fmt chunk that defines parameters for the
    /// entire file, unlike some formats that may have parameter changes
    /// at specific points. This enables optimizations in audio processing.
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    /// Returns the number of audio channels.
    ///
    /// WAV supports various channel configurations:
    /// - 1 channel: Mono
    /// - 2 channels: Stereo
    /// - 3+ channels: Multi-channel configurations (5.1, 7.1, etc.)
    ///
    /// # Guarantees
    ///
    /// The returned value is constant for the lifetime of the decoder and
    /// matches the channel count specified in the WAV file's fmt chunk.
    /// This value is available immediately upon decoder creation.
    #[inline]
    fn channels(&self) -> ChannelCount {
        self.channels
    }

    /// Returns the sample rate in Hz.
    ///
    /// # Guarantees
    ///
    /// The returned value is constant for the lifetime of the decoder.
    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    /// Returns the total duration of the audio stream.
    ///
    /// # Returns
    ///
    /// Always returns `Some(duration)` for valid WAV files.
    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        Some(self.total_duration)
    }

    /// Returns the bit depth of the audio samples.
    ///
    /// # Implementation Note
    ///
    /// Up to 24 bits of information is preserved from the original WAV file and
    /// used for proper sample scaling during conversion to Rodio's sample format.
    ///
    /// # Returns
    ///
    /// Always returns `Some(depth)` for valid WAV files. The bit depth is
    /// constant throughout the file and matches the fmt chunk specification.
    #[inline]
    fn bits_per_sample(&self) -> Option<BitDepth> {
        Some(self.bits_per_sample)
    }

    /// Attempts to seek to the specified position in the audio stream.
    ///
    /// WAV seeking is highly efficient due to the uncompressed format and
    /// direct sample access. The implementation provides both fast random
    /// access seeking and forward-only seeking based on configuration.
    ///
    /// # Seeking Modes
    ///
    /// - **Random access** (when `is_seekable`): Direct positioning using hound's seek
    ///   - O(1) performance via direct file positioning
    ///   - Sample-accurate positioning
    /// - **Forward-only** (when not `is_seekable`): Linear sample skipping
    ///   - O(n) performance where n is samples to skip
    ///   - Prevents backward seeks for streaming scenarios
    ///
    /// # Performance Characteristics
    ///
    /// - **Random access seeks**: Extremely fast, direct file positioning
    /// - **Forward seeks**: Efficient sample skipping with minimal overhead
    /// - **Channel alignment**: Preserves correct channel order after seeking
    /// - **Boundary handling**: Seeks beyond end are clamped to file length
    ///
    /// # Arguments
    ///
    /// * `pos` - Target position as duration from stream start
    ///
    /// # Errors
    ///
    /// - `SeekError::ForwardOnly` - Backward seek attempted without `is_seekable`
    /// - `SeekError::IoError` - I/O error during seek operation (rare for valid files)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::{fs::File, time::Duration};
    /// use rodio::{Decoder, Source};
    ///
    /// let file = File::open("audio.wav").unwrap();
    /// let mut decoder = Decoder::builder()
    ///     .with_data(file)
    ///     .with_seekable(true)
    ///     .build()
    ///     .unwrap();
    ///
    /// // Instant seek to 30 seconds
    /// decoder.try_seek(Duration::from_secs(30)).unwrap();
    /// ```
    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        let file_len = self.reader.reader.duration();

        let new_pos = (pos.as_secs_f64() * self.sample_rate().get() as f64) as u32;
        let new_pos = new_pos.min(file_len); // saturate pos at the end of the source

        let target_sample = new_pos * self.channels().get() as u32;
        let samples_to_skip = if !self.is_seekable {
            if target_sample < self.reader.samples_read {
                return Err(SeekError::ForwardOnly);
            } else {
                // we can only skip forward, so calculate how many samples to skip
                target_sample - self.reader.samples_read
            }
        } else {
            // seekable, so we can jump directly to the target sample
            // make sure the next sample is for the right channel
            let active_channel = self.reader.samples_read % self.channels().get() as u32;

            self.reader.reader.seek(new_pos).map_err(Arc::new)?;
            self.reader.samples_read = new_pos * self.channels().get() as u32;

            active_channel
        };

        for _ in 0..samples_to_skip {
            let _ = self.next();
        }

        Ok(())
    }
}

impl<R> Iterator for WavDecoder<R>
where
    R: Read + Seek,
{
    /// The type of samples yielded by the iterator.
    ///
    /// Returns `Sample` values representing individual audio samples.
    /// Samples are interleaved across channels in the order: channel 0, channel 1, etc.
    type Item = Sample;

    /// Returns the next audio sample from the WAV stream.
    ///
    /// This method delegates to the internal `SamplesIterator` which handles
    /// sample format conversion and position tracking. WAV iteration is highly
    /// efficient due to direct sample access without decompression.
    ///
    /// # Performance
    ///
    /// WAV sample iteration has minimal overhead:
    /// - Direct file reads without decompression
    /// - Efficient sample format conversion
    /// - No intermediate buffering required
    /// - Optimal for real-time audio processing
    ///
    /// # Returns
    ///
    /// - `Some(sample)` - Next audio sample from the WAV file
    /// - `None` - End of file reached or I/O error occurred
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.reader.next()
    }

    /// Returns bounds on the remaining amount of samples.
    ///
    /// Delegates to the internal `SamplesIterator` which provides exact
    /// remaining sample count based on WAV header information.
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.reader.size_hint()
    }
}

/// Probes input data to detect WAV format.
fn is_wave<R>(data: &mut R) -> bool
where
    R: Read + Seek,
{
    utils::probe_format(data, |reader| WavReader::new(reader).is_ok())
}
