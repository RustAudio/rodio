//! Ogg Vorbis audio decoder implementation.
//!
//! This module provides Ogg Vorbis decoding capabilities using the `lewton` library.
//! Vorbis is a lossy audio compression format contained within Ogg containers, designed
//! for high-quality audio compression with lower bitrates than MP3.
//!
//! # Features
//!
//! - **Quality**: High-quality lossy compression with advanced psychoacoustic modeling
//! - **Bitrates**: Variable bitrate encoding optimized for quality per bit
//! - **Sample rates**: 8kHz to 192kHz (commonly 44.1kHz and 48kHz)
//! - **Channels**: Mono to 8-channel surround sound support
//! - **Seeking**: Granule-based seeking with binary search optimization
//! - **Duration**: Calculated via last granule position scanning
//! - **Streaming**: Supports chained Ogg streams with parameter changes
//!
//! # Limitations
//!
//! - No bit depth detection (lossy format with floating-point processing)
//! - Duration calculation requires scanning to last granule position
//! - Seeking accuracy depends on granule position availability
//! - Forward-only seeking without `is_seekable` setting
//!
//! # Configuration
//!
//! The decoder can be configured through `DecoderBuilder`:
//! - `with_seekable(true)` - Enable backward seeking with granule search
//! - `with_scan_duration(true)` - Enable duration scanning (requires `byte_len`)
//! - `with_total_duration(dur)` - Provide known duration to skip scanning
//! - `with_seek_mode(SeekMode::Fastest)` - Use granule-based seeking for speed
//! - `with_seek_mode(SeekMode::Nearest)` - Use linear seeking for accuracy
//!
//! # Performance Notes
//!
//! - Duration scanning uses binary search optimization for large files
//! - Granule-based seeking provides O(log n) performance vs. O(n) linear
//! - Variable packet sizes require careful buffer management
//! - Chained streams may cause parameter changes requiring adaptation
//!
//! # Example
//!
//! ```ignore
//! use std::fs::File;
//! use rodio::{Decoder, decoder::builder::SeekMode};
//!
//! let file = File::open("audio.ogg").unwrap();
//! let decoder = Decoder::builder()
//!     .with_data(file)
//!     .with_seekable(true)
//!     .with_scan_duration(true)
//!     .with_seek_mode(SeekMode::Fastest)
//!     .build()
//!     .unwrap();
//!
//! // Vorbis format doesn't support bit depth detection
//! assert_eq!(decoder.bits_per_sample(), None);
//! ```

use std::{
    io::{Read, Seek, SeekFrom},
    sync::Arc,
    time::Duration,
};

use dasp_sample::Sample as _;
use lewton::{
    audio::AudioReadError::AudioIsHeader,
    inside_ogg::OggStreamReader,
    samples::InterleavedSamples,
    OggReadError::NoCapturePatternFound,
    VorbisError::{BadAudio, OggError},
};

use super::{utils, Settings};
use crate::{
    common::{BitDepth, ChannelCount, Sample, SampleRate},
    decoder::builder::SeekMode,
    math::duration_to_float,
    source::SeekError,
    Float, Source,
};

/// Decoder for Ogg Vorbis format using the `lewton` library.
///
/// Provides high-quality lossy audio decoding with granule-based seeking and duration
/// calculation through Ogg stream analysis. The decoder handles variable packet sizes
/// efficiently and supports chained Ogg streams with parameter changes.
///
/// # Granule-based Architecture
///
/// Vorbis uses granule positions as timing references, where each granule represents
/// a sample position in the decoded audio stream. This enables precise seeking and
/// duration calculation without requiring constant bitrate assumptions.
///
/// # Packet Processing
///
/// Ogg Vorbis audio is organized into variable-size packets containing compressed
/// audio data. Each packet decodes to a variable number of samples, requiring
/// dynamic buffer management for efficient sample-by-sample iteration.
///
/// # Seeking Strategies
///
/// The decoder implements two seeking approaches:
/// - **Granule seeking**: Fast binary search using lewton's native mechanism
/// - **Linear seeking**: Sample-accurate positioning via forward iteration
///
/// # Stream Chaining
///
/// Ogg supports chained streams where multiple Vorbis streams are concatenated.
/// The decoder adapts to parameter changes (sample rate, channels) between streams.
///
/// # Generic Parameters
///
/// * `R` - The underlying data source type, must implement `Read + Seek`
pub struct VorbisDecoder<R>
where
    R: Read + Seek,
{
    /// The underlying lewton Ogg stream reader, wrapped for seeking operations.
    ///
    /// Temporarily set to `None` during stream reset operations for linear seeking.
    /// Always `Some` during normal operation and iteration.
    stream_reader: Option<OggStreamReader<R>>,

    /// Current decoded audio packet data.
    ///
    /// Contains interleaved PCM samples from the current Vorbis packet. `None` indicates
    /// either stream exhaustion or that a new packet needs to be decoded. Packet sizes
    /// vary based on audio content and encoder settings.
    current_data: Option<Vec<f32>>,

    /// Current position within the current packet.
    ///
    /// Tracks the next sample index to return from the current packet's data.
    /// When this reaches the packet's sample count, a new packet must be decoded.
    current_data_offset: usize,

    /// Total duration calculated from last granule position.
    ///
    /// Calculated by scanning to the final granule position in the stream or
    /// provided explicitly via settings. For chained streams, represents the
    /// total duration across all chains.
    total_duration: Option<Duration>,

    /// Total number of audio samples (estimated from duration).
    ///
    /// Calculated from total duration and stream parameters when available.
    /// Represents total interleaved samples across all channels and chains.
    total_samples: Option<u64>,

    /// Number of samples read so far (for seeking calculations).
    ///
    /// Tracks the current playback position in total samples (across all channels).
    /// Used to determine if seeking requires stream reset or can skip forward.
    samples_read: u64,

    /// Seeking precision mode.
    ///
    /// Controls the trade-off between seeking speed and accuracy:
    /// - `Fastest`: Granule-based seeking using lewton's binary search
    /// - `Nearest`: Linear seeking for sample-accurate positioning
    seek_mode: SeekMode,

    /// Whether random access seeking is enabled.
    ///
    /// When `true`, enables backward seeking by allowing stream reset operations.
    /// When `false`, only forward seeking (sample skipping) is allowed.
    is_seekable: bool,
}

impl<R> VorbisDecoder<R>
where
    R: Read + Seek,
{
    /// Attempts to decode the data as Ogg Vorbis with default settings.
    ///
    /// This method probes the input data to detect Ogg Vorbis format and initializes
    /// the decoder if successful. Uses default settings with no seeking support or
    /// duration scanning enabled.
    ///
    /// # Arguments
    ///
    /// * `data` - Input stream implementing `Read + Seek`
    ///
    /// # Returns
    ///
    /// - `Ok(VorbisDecoder)` if the data contains valid Ogg Vorbis format
    /// - `Err(R)` if the data is not Ogg Vorbis, returning the original stream
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::fs::File;
    /// use rodio::decoder::vorbis::VorbisDecoder;
    ///
    /// let file = File::open("audio.ogg").unwrap();
    /// match VorbisDecoder::new(file) {
    ///     Ok(decoder) => println!("Vorbis decoder created"),
    ///     Err(file) => println!("Not an Ogg Vorbis file"),
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// This method performs format detection which requires parsing Ogg headers
    /// and Vorbis identification. The stream position is restored if detection fails,
    /// so the original stream can be used for other format detection attempts.
    #[allow(dead_code)]
    pub fn new(data: R) -> Result<Self, R> {
        Self::new_with_settings(data, &Settings::default())
    }

    /// Attempts to decode the data as Ogg Vorbis with custom settings.
    ///
    /// This method provides full control over decoder configuration including seeking
    /// behavior, duration calculation, and performance optimizations. It performs format
    /// detection, analyzes stream characteristics, and optionally scans for accurate
    /// duration information.
    ///
    /// # Arguments
    ///
    /// * `data` - Input stream implementing `Read + Seek`
    /// * `settings` - Configuration settings from `DecoderBuilder`
    ///
    /// # Returns
    ///
    /// - `Ok(VorbisDecoder)` if the data contains valid Ogg Vorbis format
    /// - `Err(R)` if the data is not Ogg Vorbis, returning the original stream
    ///
    /// # Settings Usage
    ///
    /// - `is_seekable`: Enables backward seeking operations
    /// - `scan_duration`: Enables granule position scanning (requires `byte_len`)
    /// - `total_duration`: Provides known duration to skip scanning
    /// - `seek_mode`: Controls seeking accuracy vs. speed trade-off
    /// - `byte_len`: Total file size used for duration scanning optimization
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::fs::File;
    /// use std::time::Duration;
    /// use rodio::decoder::{vorbis::VorbisDecoder, Settings, builder::SeekMode};
    ///
    /// let file = File::open("audio.ogg").unwrap();
    /// let mut settings = Settings::default();
    /// settings.is_seekable = true;
    /// settings.scan_duration = true;
    /// settings.seek_mode = SeekMode::Fastest;
    ///
    /// let decoder = VorbisDecoder::new_with_settings(file, &settings).unwrap();
    /// ```
    ///
    /// # Performance
    ///
    /// - Duration scanning uses binary search optimization for large files
    /// - Stream initialization requires parsing Vorbis headers and identification
    /// - First packet decoding provides immediate stream characteristics
    ///
    /// # Panics
    ///
    /// Panics if the Ogg Vorbis stream has invalid characteristics (zero channels or
    /// zero sample rate). This should never happen with valid Vorbis data that passes
    /// format detection.
    pub fn new_with_settings(mut data: R, settings: &Settings) -> Result<Self, R> {
        if !is_vorbis(&mut data) {
            return Err(data);
        }

        // Calculate total duration using the new settings approach (before consuming data)
        let mut last_granule = None;
        if settings.scan_duration && settings.is_seekable {
            if let Some(byte_len) = settings.byte_len {
                last_granule = find_last_granule(&mut data, byte_len);
            }
        }

        let mut stream_reader = OggStreamReader::new(data).expect("should still be vorbis");
        let current_data = read_next_non_empty_packet(&mut stream_reader);

        let sample_rate = SampleRate::new(stream_reader.ident_hdr.audio_sample_rate)
            .expect("vorbis has non-zero sample rate");
        let channels = stream_reader.ident_hdr.audio_channels;

        let total_duration = settings
            .total_duration
            .or_else(|| last_granule.map(|granule| granules_to_duration(granule, sample_rate)));

        let total_samples = total_duration.map(|dur| {
            let total_secs = duration_to_float(dur);
            (total_secs * sample_rate.get() as Float * channels as Float).ceil() as u64
        });

        Ok(Self {
            stream_reader: Some(stream_reader),
            current_data,
            current_data_offset: 0,
            total_duration,
            total_samples,
            samples_read: 0,
            seek_mode: settings.seek_mode,
            is_seekable: settings.is_seekable,
        })
    }

    /// Consumes the decoder and returns the underlying Ogg stream reader.
    ///
    /// This can be useful for accessing the underlying lewton stream reader directly
    /// or when the decoder needs to be replaced. The reader will be positioned at
    /// the current playback location.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::fs::File;
    /// use rodio::decoder::vorbis::VorbisDecoder;
    ///
    /// let file = File::open("audio.ogg").unwrap();
    /// let decoder = VorbisDecoder::new(file).unwrap();
    /// let stream_reader = decoder.into_inner();
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if called during a seeking operation when the stream reader is
    /// temporarily `None`. This should never happen during normal usage.
    #[inline]
    pub fn into_inner(self) -> OggStreamReader<R> {
        self.stream_reader
            .expect("stream_reader should always be Some")
    }

    /// Performs linear seeking by iteration through samples.
    ///
    /// This method provides sample-accurate seeking by linearly consuming samples
    /// until the target position is reached. It's slower than granule-based seeking
    /// but guarantees precise positioning.
    ///
    /// # Arguments
    ///
    /// * `target_granule_pos` - Target granule position (per-channel sample number)
    ///
    /// # Returns
    ///
    /// The number of samples to skip to reach exact channel alignment after seeking
    ///
    /// # Errors
    ///
    /// - `SeekError::IoError` - I/O error during stream reset operations
    ///
    /// # Performance
    ///
    /// - **Forward seeks**: O(n) where n is samples to skip
    /// - **Backward seeks**: O(target_position) due to stream reset
    /// - **Always accurate**: Guarantees sample-perfect positioning
    ///
    /// # Implementation
    ///
    /// For backward seeks, the stream is reset to the beginning and the decoder
    /// is reinitialized to ensure consistent state. Forward seeks skip samples
    /// from the current position.
    fn linear_seek(&mut self, target_granule_pos: u64) -> Result<u64, SeekError> {
        let target_samples = target_granule_pos * self.channels().get() as u64;
        let current_samples = self.samples_read;

        let samples_to_skip = if target_samples < current_samples {
            // Backwards seek: reset to start by recreating stream reader
            let mut reader = self
                .stream_reader
                .take()
                .expect("stream_reader should always be Some")
                .into_inner();

            reader.seek_bytes(SeekFrom::Start(0)).map_err(Arc::new)?;

            // Recreate stream reader and reinitialize like a fresh decoder
            let mut new_stream_reader =
                OggStreamReader::new(reader.into_inner()).map_err(Arc::new)?;

            self.current_data = read_next_non_empty_packet(&mut new_stream_reader);
            self.stream_reader = Some(new_stream_reader);
            self.current_data_offset = 0;
            self.samples_read = 0;

            // Consume exactly target_samples to position at the target
            target_samples
        } else {
            // Forward seek: skip from current position
            target_samples - current_samples
        };

        Ok(samples_to_skip)
    }

    /// Performs granule-based seeking using lewton's native mechanism.
    ///
    /// This method uses lewton's built-in binary search algorithm to quickly locate
    /// the target granule position. It's faster than linear seeking but may not be
    /// sample-accurate due to the granular nature of Ogg page boundaries.
    ///
    /// # Arguments
    ///
    /// * `target_granule_pos` - Target granule position (per-channel sample number)
    ///
    /// # Returns
    ///
    /// The number of samples to skip to reach exact positioning after coarse seek
    ///
    /// # Errors
    ///
    /// - `SeekError::LewtonDecoder` - Lewton decoder error during granule seeking
    ///
    /// # Performance
    ///
    /// - **Seeking time**: O(log n) binary search through Ogg pages
    /// - **Accuracy**: Positions at or before target granule (requires fine-tuning)
    /// - **Optimal for**: Large files with frequent seeking requirements
    fn granule_seek(&mut self, target_granule_pos: u64) -> Result<u64, SeekError> {
        let reader = self
            .stream_reader
            .as_mut()
            .expect("stream_reader should always be Some");

        // Use lewton's bisection-based granule seeking
        reader.seek_absgp_pg(target_granule_pos).map_err(Arc::new)?;

        // Clear buffer - let next() handle loading new packets
        self.current_data = None;

        // Update samples_read to reflect approximate new position (interleaved samples)
        // In ogg 0.9.2 get_last_absgp always returns 0: https://github.com/RustAudio/ogg/pull/22
        let current_granule_pos = reader.get_last_absgp().unwrap_or(target_granule_pos);
        self.samples_read = current_granule_pos * self.channels().get() as u64;

        // lewton does not seek to the exact position, it seeks to a granule position at or before.
        let samples_to_skip =
            target_granule_pos.saturating_sub(current_granule_pos) * self.channels().get() as u64;

        Ok(samples_to_skip)
    }
}

impl<R> Source for VorbisDecoder<R>
where
    R: Read + Seek,
{
    /// Returns the number of samples before parameters change.
    ///
    /// # Chained Streams
    ///
    /// Ogg supports chained streams where multiple Vorbis streams are concatenated.
    /// When stream parameters change (sample rate, channels), the span length
    /// reflects the current stream's characteristics.
    ///
    /// # Returns
    ///
    /// This returns `Some(packet_size)` when a packet is available, representing the
    /// number of samples in the current packet. Returns `Some(0)` when the stream is
    /// exhausted.
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        // Chained Ogg streams are supported by lewton, so parameters can change.
        // Return current buffer length, or Some(0) when exhausted.
        self.current_data
            .as_ref()
            .map(|data| data.len())
            .or(Some(0))
    }

    /// Returns the number of audio channels.
    ///
    /// Ogg Vorbis supports various channel configurations:
    /// - 1 channel: Mono
    /// - 2 channels: Stereo
    /// - 3 channels: Stereo + center
    /// - 4 channels: Quadraphonic
    /// - 5 channels: 5.0 surround
    /// - 6 channels: 5.1 surround
    /// - 7 channels: 6.1 surround
    /// - 8 channels: 7.1 surround
    ///
    /// # Chained Streams
    ///
    /// In chained Ogg streams, channel configuration can change between streams.
    /// The decoder adapts to these changes automatically, though this is uncommon
    /// in practice.
    ///
    /// # Guarantees
    ///
    /// The returned value reflects the current stream's channel configuration and
    /// may change during playback if chained streams with different parameters
    /// are encountered.
    #[inline]
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(
            self.stream_reader
                .as_ref()
                .expect("stream_reader should always be Some")
                .ident_hdr
                .audio_channels
                .into(),
        )
        .expect("audio should have at least one channel")
    }

    /// Returns the sample rate in Hz.
    ///
    /// # Chained Streams
    ///
    /// Sample rate can change between chained streams, though this is rare in
    /// practice. The decoder handles such changes automatically.
    ///
    /// # Guarantees
    ///
    /// The returned value reflects the current stream's sample rate and may change
    /// during playback if chained streams with different parameters are encountered.
    #[inline]
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(
            self.stream_reader
                .as_ref()
                .expect("stream_reader should always be Some")
                .ident_hdr
                .audio_sample_rate,
        )
        .expect("audio should always have a non zero SampleRate")
    }

    /// Returns the total duration of the audio stream.
    ///
    /// # Chained Streams
    ///
    /// For chained streams, duration represents the total across all chains.
    /// Individual chain durations are not separately tracked.
    ///
    /// # Returns
    ///
    /// Returns `None` when scanning is disabled or prerequisites are not met.
    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }

    /// Returns the bit depth of the audio samples.
    ///
    /// # Returns
    ///
    /// This method always returns `None` for Vorbis streams as bit depth is not
    /// a meaningful concept for lossy compressed audio formats.
    #[inline]
    fn bits_per_sample(&self) -> Option<BitDepth> {
        None
    }

    /// Attempts to seek to the specified position in the audio stream.
    ///
    /// Ogg Vorbis seeking uses granule positions for precise timing references.
    /// The implementation provides both fast granule-based seeking and slower
    /// but sample-accurate linear seeking.
    ///
    /// # Seeking Modes
    ///
    /// - **`SeekMode::Fastest`**: Uses lewton's granule-based binary search
    ///   - Fast O(log n) performance for large files
    ///   - May require fine-tuning for exact positioning
    /// - **`SeekMode::Nearest`**: Uses linear sample consumption
    ///   - Slower O(n) performance but always sample-accurate
    ///   - Guarantees exact positioning regardless of granule boundaries
    ///
    /// # Performance Characteristics
    ///
    /// - **Granule seeks**: Fast for large files, optimal for frequent seeking
    /// - **Linear seeks**: Slower but always accurate, good for precise positioning
    /// - **Forward seeks**: Efficient skipping from current position
    /// - **Backward seeks**: Requires stream reset, then forward positioning
    ///
    /// # Arguments
    ///
    /// * `pos` - Target position as duration from stream start
    ///
    /// # Errors
    ///
    /// - `SeekError::ForwardOnly` - Backward seek attempted without `is_seekable`
    /// - `SeekError::LewtonDecoder` - Lewton decoder error during granule seeking
    /// - `SeekError::IoError` - I/O error during stream reset or positioning
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::{fs::File, time::Duration};
    /// use rodio::{Decoder, Source, decoder::builder::SeekMode};
    ///
    /// let file = File::open("audio.ogg").unwrap();
    /// let mut decoder = Decoder::builder()
    ///     .with_data(file)
    ///     .with_seekable(true)
    ///     .with_seek_mode(SeekMode::Fastest)
    ///     .build()
    ///     .unwrap();
    ///
    /// // Fast granule-based seek to 30 seconds
    /// decoder.try_seek(Duration::from_secs(30)).unwrap();
    /// ```
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
        let active_channel = self.current_data_offset % self.channels().get() as usize;

        // Convert duration to granule position (per-channel sample number)
        // lewton's seek_absgp_pg expects absolute granule position
        let target_granule_pos =
            (duration_to_float(target) * self.sample_rate().get() as Float) as u64;
        let target_sample = target_granule_pos * self.channels().get() as u64;

        let samples_to_skip = if !self.is_seekable {
            if target_sample < self.samples_read {
                return Err(SeekError::ForwardOnly);
            } else {
                // Linearly consume samples to reach forward targets
                target_sample - self.samples_read
            }
        } else if self.seek_mode == SeekMode::Nearest {
            self.linear_seek(target_granule_pos)?
        } else {
            self.granule_seek(target_granule_pos)?
        };

        // After seeking, we're always positioned at the start of an audio frame (channel 0).
        // Skip samples to reach the desired channel position.
        for _ in 0..(samples_to_skip + active_channel as u64) {
            let _ = self.next();
        }

        Ok(())
    }
}

impl<R> Iterator for VorbisDecoder<R>
where
    R: Read + Seek,
{
    /// The type of samples yielded by the iterator.
    ///
    /// Returns `Sample` values representing individual audio samples.
    /// Samples are interleaved across channels in the order: channel 0, channel 1, etc.
    type Item = Sample;

    /// Returns the next audio sample from the Ogg Vorbis stream.
    ///
    /// This method implements efficient packet-based decoding by maintaining the current
    /// decoded Vorbis packet and returning samples one at a time. It automatically decodes
    /// new packets as needed and handles various Ogg/Vorbis stream conditions.
    ///
    /// # Performance
    ///
    /// - **Hot path**: Returning samples from current packet (very fast)
    /// - **Cold path**: Decoding new packets when buffer is exhausted (slower)
    ///
    /// # Error Handling
    ///
    /// The decoder gracefully handles various Ogg/Vorbis stream conditions:
    /// - **Header packets**: Automatically skipped during audio playback
    /// - **Empty packets**: Ignored, decoder continues to next packet
    /// - **Stream errors**: Most errors result in stream termination
    /// - **Capture pattern errors**: Handled for robust stream processing
    ///
    /// # Returns
    ///
    /// - `Some(sample)` - Next audio sample from the stream
    /// - `None` - End of stream reached or unrecoverable decoding error
    ///
    /// # Channel Order
    ///
    /// Samples are returned in interleaved order based on Vorbis channel mapping:
    /// - **Mono**: [M, M, M, ...]
    /// - **Stereo**: [L, R, L, R, ...]
    /// - **5.1 Surround**: [FL, FR, C, LFE, BL, BR, FL, FR, C, LFE, BL, BR, ...]
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Hot path: read from current buffer if available
        if let Some(data) = &self.current_data {
            if self.current_data_offset < data.len() {
                let sample = data[self.current_data_offset];
                self.current_data_offset += 1;
                self.samples_read += 1;
                #[cfg(feature = "64bit")]
                let sample = sample.to_sample();
                return Some(sample);
            }
        }

        // Cold path: need to decode next packet
        let stream_reader = self
            .stream_reader
            .as_mut()
            .expect("stream_reader should always be Some");

        if let Some(samples) = read_next_non_empty_packet(stream_reader) {
            self.current_data = Some(samples);
            self.current_data_offset = 0;

            // Return first sample from new buffer
            if let Some(data) = &self.current_data {
                let sample = data[0];
                self.current_data_offset = 1;
                self.samples_read += 1;
                #[cfg(feature = "64bit")]
                let sample = sample.to_sample();
                return Some(sample);
            }
        }

        // Stream exhausted - set buffer to None
        self.current_data = None;
        None
    }

    /// Returns bounds on the remaining amount of samples.
    ///
    /// Provides size estimates based on Ogg Vorbis stream characteristics and current
    /// playback position. The accuracy depends on the availability of duration information
    /// from granule position scanning or explicit duration settings.
    ///
    /// # Accuracy Levels
    ///
    /// - **High accuracy**: When total samples calculated from granule scanning
    /// - **Conservative estimate**: When only current packet information available
    /// - **Stream exhausted**: (0, Some(0)) when no more data
    ///
    /// # Implementation
    ///
    /// The lower bound represents samples currently buffered in the decoded packet.
    /// The upper bound uses total sample estimates when available, providing useful
    /// information for progress indication and buffer allocation.
    ///
    /// # Use Cases
    ///
    /// - **Progress indication**: Upper bound enables percentage calculation
    /// - **Buffer allocation**: Lower bound ensures minimum available samples
    /// - **End detection**: (0, Some(0)) indicates stream completion
    ///
    /// # Chained Streams
    ///
    /// For chained streams, estimates represent the remaining samples across all
    /// remaining chains in the file.
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        // Samples already decoded and buffered (guaranteed available)
        let buffered_samples = self
            .current_data
            .as_ref()
            .map(|data| data.len().saturating_sub(self.current_data_offset))
            .unwrap_or(0);

        if let Some(total_samples) = self.total_samples {
            let total_remaining = total_samples.saturating_sub(self.samples_read) as usize;
            (buffered_samples, Some(total_remaining))
        } else if self.current_data.is_none() {
            // Stream exhausted
            (0, Some(0))
        } else {
            (buffered_samples, None)
        }
    }
}

/// Reads the next non-empty packet with proper error handling.
///
/// This function handles the complexity of Ogg Vorbis packet reading, filtering out
/// header packets and empty audio packets while gracefully handling various error
/// conditions that can occur during stream processing.
///
/// # Arguments
///
/// * `stream_reader` - Mutable reference to the lewton OggStreamReader
///
/// # Returns
///
/// - `Some(samples)` - Vector of interleaved audio samples from the next valid packet
/// - `None` - Stream exhausted or unrecoverable error occurred
fn read_next_non_empty_packet<R: Read + Seek>(
    stream_reader: &mut OggStreamReader<R>,
) -> Option<Vec<f32>> {
    loop {
        match stream_reader.read_dec_packet_generic::<InterleavedSamples<f32>>() {
            Ok(Some(packet)) => {
                // Only accept packets with actual audio samples
                if !packet.samples.is_empty() {
                    return Some(packet.samples);
                }
                // Empty packet - continue to next one
                continue;
            }
            Ok(None) => {
                // Stream exhausted
                return None;
            }

            // Ignore header-related errors and continue
            Err(BadAudio(AudioIsHeader)) => continue,
            Err(OggError(NoCapturePatternFound)) => continue,

            // All other errors are terminal
            Err(_) => return None,
        }
    }
}

/// Finds the last granule position in an Ogg Vorbis stream using optimized scanning.
///
/// This function implements an efficient algorithm to locate the final granule position
/// in an Ogg stream, which represents the total number of audio samples. It uses binary
/// search optimization to minimize I/O operations for large files.
///
/// # Arguments
///
/// * `data` - Mutable reference to the input stream to scan
/// * `byte_len` - Total file size used for binary search optimization
///
/// # Returns
///
/// - `Some(granule_pos)` - The final granule position if found
/// - `None` - If scanning failed or no valid granule positions found
///
/// # Algorithm
///
/// 1. **Binary search phase**: Quickly locate a region containing granule positions
/// 2. **Linear scan phase**: Thoroughly scan from the optimized start position
/// 3. **Position restoration**: Return stream to original position
///
/// # Performance
///
/// - **Large files**: Significantly faster than linear scanning
/// - **Small files**: Minimal overhead compared to linear scanning
/// - **I/O optimization**: Reduces read operations through intelligent positioning
///
/// # Implementation Details
///
/// The binary search phase stops when the search range is smaller than 4KB, at which
/// point a final linear scan ensures all granule positions are found. The packet
/// limit during binary search prevents excessive scanning in dense regions.
fn find_last_granule<R: Read + Seek>(data: &mut R, byte_len: u64) -> Option<u64> {
    // Save current position
    let original_pos = data.stream_position().unwrap_or_default();
    let _ = data.rewind();

    // Binary search through byte positions to find optimal start position
    let mut left = 0;
    let mut right = byte_len;
    let mut best_start_position = 0;
    while right - left > 4096 {
        // Stop when range is small enough
        let mid = left + (right - left) / 2;

        // Try to find a granule from this position (limited packet scan during binary search)
        match find_granule_from_position(data, mid, Some(50)) {
            Some(_granule) => {
                // Found a granule, this means there's content at or after this position
                best_start_position = mid;
                left = mid; // Search in the right half
            }
            None => {
                // No granule found, search in the left half
                right = mid;
            }
        }
    }

    // Now do the final linear scan from the optimized start position (no packet limit)
    let result = find_granule_from_position(data, best_start_position, None);

    // Restore original position
    let _ = data.seek(SeekFrom::Start(original_pos));

    result
}

/// Finds granule positions by scanning forward from a specific byte position.
///
/// This function scans forward through Ogg packets from a given byte position to find
/// valid granule positions. It's used both during binary search optimization and for
/// final linear scanning to locate the last granule position.
///
/// # Arguments
///
/// * `data` - The data source to read from
/// * `start_pos` - Starting byte position in the file
/// * `max_packets` - Maximum packets to read before giving up (None = scan until end)
///
/// # Returns
///
/// - `Some(granule_pos)` - The last valid granule position found in the scan
/// - `None` - If no valid granule positions were found or I/O error occurred
///
/// # Packet Limit Rationale
///
/// When used during binary search, the packet limit prevents excessive scanning:
/// - **Typical Ogg pages**: Contain 1-10 packets depending on content
/// - **50 packet limit**: Covers roughly 5-50 pages (~20-400KB depending on bitrate)
/// - **Balance**: Finding granules quickly vs. avoiding excessive I/O during binary search
/// - **Final scan**: No limit ensures complete coverage from optimized position
///
/// # Granule Position Validation
///
/// The function validates granule positions by:
/// - Checking for the "unset" marker (0xFFFFFFFFFFFFFFFF)
/// - Ensuring positions are greater than 0
/// - Only considering end-of-page packets for granule information
///
/// # Performance
///
/// Scanning performance depends on:
/// - **Bitrate**: Lower bitrates have larger packets, fewer reads needed
/// - **Content**: Complex audio may have more variable packet sizes
/// - **Position**: Later positions in file may scan less data
fn find_granule_from_position<R: Read + Seek>(
    data: &mut R,
    start_pos: u64,
    max_packets: Option<usize>,
) -> Option<u64> {
    if data.seek(SeekFrom::Start(start_pos)).is_err() {
        return None;
    }

    let mut packet_reader = ogg::PacketReader::new(data.by_ref());
    let mut last_granule = None;
    let mut packets_read = 0;

    // Scan forward from start position to find granules
    while let Ok(Some(packet)) = packet_reader.read_packet() {
        if packet.last_in_page() {
            let granule = packet.absgp_page();
            // Check if granule position is valid (not unset marker and greater than 0)
            // 0xFFFFFFFFFFFFFFFF is the "unset" marker in Ogg specification
            if granule != 0xFFFFFFFFFFFFFFFF && granule > 0 {
                last_granule = Some(granule);
            }
        }

        packets_read += 1;

        // Stop if we've hit the packet limit (used during binary search)
        if let Some(max) = max_packets {
            if packets_read >= max {
                break;
            }
        }
    }

    last_granule
}

/// Calculates duration from granule position and sample rate.
///
/// This function converts a granule position (which represents the total number of
/// audio samples in a Vorbis stream) to a precise duration value. It provides
/// sample-accurate timing information for duration calculation and seeking operations.
///
/// # Arguments
///
/// * `granules` - The granule position representing total audio samples
/// * `sample_rate` - The sample rate of the audio stream
///
/// # Returns
///
/// A `Duration` representing the exact time corresponding to the granule position
///
/// # Precision
///
/// The calculation provides nanosecond precision by:
/// 1. Calculating whole seconds from sample count
/// 2. Computing remainder samples for sub-second precision
/// 3. Converting remainder to nanoseconds based on sample rate
///
/// # Implementation
///
/// This is used specifically for Ogg-based formats where granule position
/// represents the total number of samples, providing more accurate timing
/// than bitrate-based estimations.
fn granules_to_duration(granules: u64, sample_rate: SampleRate) -> Duration {
    let sample_rate = sample_rate.get() as u64;
    let secs = granules / sample_rate;
    let nanos = ((granules % sample_rate) * 1_000_000_000) / sample_rate;
    Duration::new(secs, nanos as u32)
}

/// Probes input data to detect Ogg Vorbis format.
fn is_vorbis<R>(data: &mut R) -> bool
where
    R: Read + Seek,
{
    utils::probe_format(data, |reader| OggStreamReader::new(reader).is_ok())
}
