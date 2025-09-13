//! MP3 audio decoder implementation.
//!
//! This module provides MP3 decoding capabilities using the `minimp3` library. MP3 is a
//! lossy audio compression format that achieves smaller file sizes by removing audio
//! information that is less audible to human hearing.
//!
//! # Features
//!
//! - **MPEG layers**: Supports MPEG-1/2 Layer III (MP3)
//! - **Bitrates**: Variable and constant bitrate encoding (32-320 kbps)
//! - **Sample rates**: 8kHz to 48kHz (MPEG-1: 32/44.1/48kHz, MPEG-2: 16/22.05/24kHz)
//! - **Channels**: Mono, stereo, joint stereo, and dual channel
//! - **Seeking**: Coarse seeking with optional duration scanning
//! - **Duration**: Calculated via file scanning or metadata when available
//!
//! # Limitations
//!
//! - No bit depth detection (lossy format with dynamic range compression)
//! - Seeking accuracy depends on bitrate variability (VBR vs CBR)
//! - Forward-only seeking without `is_seekable` setting
//! - Duration scanning requires full file analysis for accuracy
//!
//! # Configuration
//!
//! The decoder can be configured through `DecoderBuilder`:
//! - `with_seekable(true)` - Enable backward seeking
//! - `with_scan_duration(true)` - Enable duration scanning (requires `byte_len`)
//! - `with_total_duration(dur)` - Provide known duration to skip scanning
//! - `with_seek_mode(SeekMode::Fastest)` - Use fastest seeking method
//!
//! # Performance Notes
//!
//! - Duration scanning can be slow for large files
//! - Variable bitrate files may have less accurate seeking
//! - Frame-based decoding minimizes memory usage
//!
//! # Example
//!
//! ```ignore
//! use std::fs::File;
//! use rodio::Decoder;
//!
//! let file = File::open("audio.mp3").unwrap();
//! let decoder = Decoder::builder()
//!     .with_data(file)
//!     .with_seekable(true)
//!     .with_scan_duration(true)
//!     .build()
//!     .unwrap();
//!
//! // MP3 format doesn't support bit depth detection
//! assert_eq!(decoder.bits_per_sample(), None);
//! ```

use std::{
    io::{Read, Seek, SeekFrom},
    sync::Arc,
    time::Duration,
};

use dasp_sample::Sample as _;

use minimp3::{Decoder, Frame};
use minimp3_fixed as minimp3;

use super::{utils, Settings};
use crate::{
    common::{BitDepth, ChannelCount, Sample, SampleRate},
    decoder::builder::SeekMode,
    math::duration_to_float,
    source::SeekError,
    Float, Source,
};

/// Decoder for the MP3 format using the `minimp3` library.
///
/// Provides lossy audio decoding with frame-based processing and coarse seeking support.
/// Duration calculation may require file scanning for accurate results with variable bitrate files.
///
/// # Frame-based Processing
///
/// MP3 audio is organized into variable-size frames containing compressed audio data.
/// Each frame is decoded independently, containing 384 (Layer I) or 1152 (Layer II/III)
/// samples per channel. The decoder maintains the current frame and tracks position
/// within it for efficient sample-by-sample iteration.
///
/// # Seeking Behavior
///
/// MP3 seeking accuracy depends on the encoding type:
/// - **Constant Bitrate (CBR)**: Accurate byte-position-based seeking
/// - **Variable Bitrate (VBR)**: Approximate seeking with potential drift
/// - **Average Bitrate (ABR)**: Moderate accuracy depending on variation
///
/// # Bitrate Adaptation
///
/// The decoder tracks average bitrate over time to improve seeking accuracy,
/// especially for VBR files where initial estimates may be inaccurate.
///
/// # Channel Changes
///
/// MP3 frames can theoretically change channel configuration, though this is
/// rare in practice. The decoder handles such changes dynamically.
///
/// # Generic Parameters
///
/// * `R` - The underlying data source type, must implement `Read + Seek`
pub struct Mp3Decoder<R>
where
    R: Read + Seek,
{
    /// The underlying minimp3 decoder, wrapped in Option to allow owning the
    /// Decoder instance during seeking.
    decoder: Option<Decoder<R>>,

    /// Byte position where audio data begins (after headers/metadata).
    ///
    /// Used as the base offset for seeking calculations. Accounts for ID3 tags,
    /// XING headers, and other metadata that precedes the actual audio frames.
    start_byte: u64,

    /// Current decoded MP3 frame (what minimp3 calls frames, rodio calls spans).
    ///
    /// Contains the raw PCM samples from the current frame. `None` indicates
    /// either stream exhaustion or that a new frame needs to be decoded.
    current_span: Option<Frame>,

    /// Current position within the current frame.
    ///
    /// Tracks the next sample index to return from the current frame's data.
    /// When this reaches the frame's sample count, a new frame must be decoded.
    current_span_offset: usize,

    /// Number of audio channels.
    ///
    /// Can theoretically change between frames, though this is rare in practice.
    /// Updated dynamically when frame channel count changes.
    channels: ChannelCount,

    /// Sample rate in Hz.
    ///
    /// Does not change after decoder initialization.
    sample_rate: SampleRate,

    /// Number of samples read so far (for seeking calculations).
    ///
    /// Tracks the current playback position in total samples (across all channels).
    /// Used to determine if seeking requires stream reset or can skip forward.
    samples_read: u64,

    /// Total number of audio samples (estimated from duration).
    ///
    /// Calculated from total duration when available. For VBR files without
    /// metadata, this may be an estimate based on average bitrate calculations.
    total_samples: Option<u64>,

    /// Total duration calculated from file analysis or metadata.
    ///
    /// Can be provided explicitly, calculated via duration scanning, or estimated
    /// from file size and average bitrate. Most accurate when obtained through
    /// full file scanning.
    total_duration: Option<Duration>,

    /// Average bitrate in bytes per second (estimated).
    ///
    /// Updated dynamically during playback to improve seeking accuracy.
    /// Initial value comes from first frame or duration/size calculation.
    average_bitrate: u32,

    /// MPEG layer (typically 3 for MP3).
    ///
    /// Determines frame structure and sample count per frame:
    /// - Layer I: 384 samples per frame
    /// - Layer II/III: 1152 samples per frame
    mpeg_layer: usize,

    /// Seeking precision mode.
    ///
    /// Controls the trade-off between seeking speed and accuracy:
    /// - `Fastest`: Byte-position-based seeking (fastest but potentially inaccurate for VBR)
    /// - `Nearest`: Sample-accurate seeking (slower but always accurate)
    seek_mode: SeekMode,

    /// Whether random access seeking is enabled.
    ///
    /// When `true`, enables backward seeking by allowing stream reset operations.
    /// When `false`, only forward seeking (sample skipping) is allowed.
    is_seekable: bool,
}

impl<R> Mp3Decoder<R>
where
    R: Read + Seek,
{
    /// Attempts to decode the data as MP3 with default settings.
    ///
    /// This method probes the input data to detect MP3 format and initializes the decoder if
    /// successful. Uses default settings with no seeking support or duration scanning enabled.
    ///
    /// # Arguments
    ///
    /// * `data` - Input stream implementing `Read + Seek`
    ///
    /// # Returns
    ///
    /// - `Ok(Mp3Decoder)` if the data contains valid MP3 format
    /// - `Err(R)` if the data is not MP3, returning the original stream
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::fs::File;
    /// use rodio::decoder::mp3::Mp3Decoder;
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// match Mp3Decoder::new(file) {
    ///     Ok(decoder) => println!("MP3 decoder created"),
    ///     Err(file) => println!("Not an MP3 file"),
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// This method performs format detection which requires decoding the first MP3 frame.
    /// The stream position is restored if detection fails, so the original stream
    /// can be used for other format detection attempts.
    #[allow(dead_code)]
    pub fn new(data: R) -> Result<Self, R> {
        Self::new_with_settings(data, &Settings::default())
    }

    /// Attempts to decode the data as MP3 with custom settings.
    ///
    /// This method provides full control over decoder configuration including seeking behavior,
    /// duration calculation, and performance optimizations. It performs format detection,
    /// analyzes the first frame for stream characteristics, and optionally scans the entire
    /// file for accurate duration information.
    ///
    /// # Arguments
    ///
    /// * `data` - Input stream implementing `Read + Seek`
    /// * `settings` - Configuration settings from `DecoderBuilder`
    ///
    /// # Returns
    ///
    /// - `Ok(Mp3Decoder)` if the data contains valid MP3 format
    /// - `Err(R)` if the data is not MP3, returning the original stream
    ///
    /// # Settings Usage
    ///
    /// - `is_seekable`: Enables backward seeking operations
    /// - `scan_duration`: Enables full file duration analysis (requires `byte_len`)
    /// - `total_duration`: Provides known duration to skip scanning
    /// - `seek_mode`: Controls seeking accuracy vs. speed trade-off
    /// - `byte_len`: Total file size used for bitrate and duration calculations
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use std::fs::File;
    /// use std::time::Duration;
    /// use rodio::decoder::{mp3::Mp3Decoder, Settings, builder::SeekMode};
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let mut settings = Settings::default();
    /// settings.is_seekable = true;
    /// settings.scan_duration = true;
    /// settings.seek_mode = SeekMode::Fastest;
    ///
    /// let decoder = Mp3Decoder::new_with_settings(file, &settings).unwrap();
    /// ```
    ///
    /// # Performance
    ///
    /// - Duration scanning can significantly slow initialization for large files
    /// - Bitrate calculation accuracy improves with `byte_len` availability
    /// - First frame analysis provides immediate stream characteristics
    ///
    /// # Panics
    ///
    /// Panics if the MP3 stream has invalid characteristics (zero channels or zero sample rate).
    /// This should never happen with valid MP3 data that passes format detection.
    pub fn new_with_settings(mut data: R, settings: &Settings) -> Result<Self, R> {
        if !is_mp3(&mut data) {
            return Err(data);
        }

        // Calculate total duration using the new settings approach
        let total_duration = if let Some(duration) = settings.total_duration {
            // Use provided duration (highest priority)
            Some(duration)
        } else if settings.scan_duration && settings.is_seekable && settings.byte_len.is_some() {
            // All prerequisites met - try scanning
            get_mp3_duration(&mut data)
        } else {
            // Either scanning disabled or prerequisites not met
            None
        };

        let mut decoder = Decoder::new(data);

        let current_span = decoder.next_frame().expect("should still be mp3");
        let channels = current_span.channels;
        let sample_rate = current_span.sample_rate;
        let mpeg_layer = current_span.layer;

        // Calculate total samples if we have duration
        let total_samples = total_duration.map(|dur| {
            (duration_to_float(dur) * sample_rate as Float * channels as Float).ceil() as u64
        });

        // Calculate the start of audio data in bytes (after MP3 headers).
        // We're currently positioned after reading the first frame, so we can
        // approximate the start of audio data by subtracting the frame size in bytes.
        let frame_samples = current_span.data.len();
        let frame_duration_secs =
            frame_samples as Float / (sample_rate as Float * channels as Float);
        let initial_bitrate_from_frame = current_span.bitrate as u32 * 1000 / 8;
        let frame_size_bytes = (frame_duration_secs * initial_bitrate_from_frame as Float) as u64;
        let start_byte = decoder
            .reader_mut()
            .stream_position()
            .map_or(0, |pos| pos.saturating_sub(frame_size_bytes));

        // Calculate average bitrate using byte_len when available
        let average_bitrate = if let (Some(duration), Some(byte_len)) =
            (total_duration, settings.byte_len)
        {
            let total_duration_secs = duration_to_float(duration);
            if total_duration_secs > 0.0 {
                let calculated = ((byte_len - start_byte) as Float / total_duration_secs) as u32;
                // Clamp average bitrate to reasonable MP3 ranges
                if mpeg_layer == 3 {
                    // 32 to 320 kbps for MPEG-1 Layer III
                    calculated.clamp(4_000, 40_000)
                } else {
                    // 32 to 384 kbps for MPEG-1 Layer I or II
                    calculated.clamp(4_000, 48_000)
                }
            } else {
                initial_bitrate_from_frame
            }
        } else {
            // No byte_len available, will use simple averaging during decode
            initial_bitrate_from_frame
        };

        Ok(Self {
            decoder: Some(decoder),
            start_byte,
            current_span: Some(current_span),
            current_span_offset: 0,
            channels: ChannelCount::new(channels as _).expect("mp3's have at least one channel"),
            sample_rate: SampleRate::new(sample_rate as _)
                .expect("mp3's have a non zero sample rate"),
            samples_read: 0,
            total_samples,
            total_duration,
            average_bitrate,
            mpeg_layer,
            seek_mode: settings.seek_mode,
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
    /// use rodio::decoder::mp3::Mp3Decoder;
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let decoder = Mp3Decoder::new(file).unwrap();
    /// let recovered_file = decoder.into_inner();
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if called during a seeking operation when the decoder is temporarily `None`.
    /// This should never happen during normal usage.
    #[inline]
    pub fn into_inner(self) -> R {
        self.decoder
            .expect("decoder should always be Some")
            .into_inner()
    }

    /// Calculates an approximate byte offset for seeking to a target sample position.
    ///
    /// This method estimates the byte position in the stream that corresponds to the
    /// target sample count using average bitrate and MPEG layer information.
    /// The accuracy depends on bitrate consistency throughout the file.
    ///
    /// # Arguments
    ///
    /// * `target_samples` - The target sample position (interleaved across all channels)
    ///
    /// # Returns
    ///
    /// Estimated byte offset from the start of the file
    ///
    /// # Accuracy
    ///
    /// - **CBR files**: High accuracy due to consistent frame sizes
    /// - **VBR files**: Moderate accuracy, may require fine-tuning after seeking
    /// - **ABR files**: Accuracy depends on actual vs. average bitrate variance
    ///
    /// # Implementation
    ///
    /// The calculation is based on:
    /// 1. Samples per frame (determined by MPEG layer)
    /// 2. Average frame size (calculated from bitrate and sample rate)
    /// 3. Number of frames to skip to reach target sample
    ///
    /// This provides a good starting point for byte-based seeking, though sample-accurate
    /// positioning may require additional fine-tuning after the seek operation.
    fn approx_byte_offset(&self, target_samples: u64) -> u64 {
        let samples_per_frame = if self.mpeg_layer == 1 {
            // MPEG-1 Layer I
            384
        } else {
            // MPEG-1 Layer II or III
            1152
        };
        let samples_per_frame_total = samples_per_frame * self.channels().get() as u64;

        let frames_to_skip = target_samples / samples_per_frame_total;

        // average frame size in bytes
        let avg_frame_size = (self.average_bitrate as Float * samples_per_frame as Float
            / self.sample_rate().get() as Float) as u64;

        self.start_byte + frames_to_skip * avg_frame_size
    }
}

impl<R> Source for Mp3Decoder<R>
where
    R: Read + Seek,
{
    /// Returns the number of samples before parameters change.
    ///
    /// For MP3, this returns `Some(frame_size)` when a frame is available, representing
    /// the number of samples in the current frame before needing to decode the next one.
    /// Returns `Some(0)` when the stream is exhausted.
    ///
    /// # Channel Changes
    ///
    /// While MP3 frames can theoretically change channel configuration, this is
    /// extremely rare in practice. Most MP3 files maintain consistent channel
    /// configuration throughout.
    ///
    /// # Frame Sizes
    ///
    /// Frame sizes depend on the MPEG layer:
    /// - Layer I: 384 samples per channel
    /// - Layer II/III: 1152 samples per channel
    ///
    /// Total samples per frame = samples_per_channel × channel_count
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        // Channel mode can change between MP3 frames. Return Some(0) when exhausted.
        self.current_span
            .as_ref()
            .map(|span| span.data.len())
            .or(Some(0))
    }

    /// Returns the number of audio channels.
    ///
    /// MP3 supports various channel configurations:
    /// - 1 channel: Mono
    /// - 2 channels: Stereo, Joint Stereo, or Dual Channel
    ///
    /// # Dynamic Changes
    ///
    /// While the MP3 specification allows channel changes between frames,
    /// this is rarely used in practice. When it does occur, the decoder
    /// updates this value automatically.
    ///
    /// # Guarantees
    ///
    /// The returned value reflects the current frame's channel configuration
    /// and may change during playback if the MP3 file uses variable channel modes.
    #[inline]
    fn channels(&self) -> ChannelCount {
        self.channels
    }

    /// Returns the sample rate in Hz.
    ///
    /// # Guarantees
    ///
    /// The sample rate is fixed for the entire MP3 stream and cannot change
    /// between frames, unlike the channel configuration.
    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    /// Returns the total duration of the audio stream.
    ///
    /// Duration accuracy depends on how it was calculated:
    /// - **Provided explicitly**: Most accurate (when available from metadata)
    /// - **File scanning**: Very accurate but slow during initialization
    /// - **Bitrate estimation**: Approximate, especially for VBR files
    /// - **Not available**: Returns `None` when duration cannot be determined
    ///
    /// # Availability
    ///
    /// Duration is available when:
    /// 1. Explicitly provided via `total_duration` setting
    /// 2. Calculated via duration scanning (when enabled and prerequisites met)
    /// 3. Estimated from file size and average bitrate (when `byte_len` available)
    ///
    /// # Returns
    ///
    /// Returns `None` when insufficient information is available for estimation.
    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }

    /// Returns the bit depth of the audio samples.
    ///
    /// # Returns
    ///
    /// This method always returns `None` for MP3 streams as bit depth is not
    /// a meaningful concept for lossy compressed audio formats.
    #[inline]
    fn bits_per_sample(&self) -> Option<BitDepth> {
        None
    }

    /// Attempts to seek to the specified position in the audio stream.
    ///
    /// MP3 seeking behavior varies based on the configured seek mode and bitrate type.
    /// The implementation balances speed and accuracy based on user preferences.
    ///
    /// # Seeking Modes
    ///
    /// - **`SeekMode::Fastest`**: Uses byte-position estimation for quick seeks
    ///   - Fast for CBR files with consistent frame sizes
    ///   - May be inaccurate for VBR files requiring fine-tuning
    /// - **`SeekMode::Nearest`**: Guarantees sample-accurate positioning
    ///   - Slower due to linear sample consumption
    ///   - Always accurate regardless of bitrate variability
    ///
    /// # Performance Characteristics
    ///
    /// - **Forward seeks**: O(1) for byte-seeking, O(n) for sample-accurate
    /// - **Backward seeks**: Requires stream reset, then forward positioning
    /// - **CBR files**: Fast and accurate byte-based seeking
    /// - **VBR files**: May require sample-accurate mode for precision
    ///
    /// # Arguments
    ///
    /// * `pos` - Target position as duration from stream start
    ///
    /// # Errors
    ///
    /// - `SeekError::ForwardOnly` - Backward seek attempted without `is_seekable`
    /// - `SeekError::IoError` - I/O error during stream reset or positioning
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::{fs::File, time::Duration};
    /// use rodio::{Decoder, Source, decoder::builder::SeekMode};
    ///
    /// let file = File::open("audio.mp3").unwrap();
    /// let mut decoder = Decoder::builder()
    ///     .with_data(file)
    ///     .with_seekable(true)
    ///     .with_seek_mode(SeekMode::Fastest)
    ///     .build()
    ///     .unwrap();
    ///
    /// // Quick seek to 30 seconds (may be approximate for VBR)
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
        let active_channel = self.current_span_offset % self.channels().get() as usize;

        // Convert duration to sample number (interleaved samples)
        let target_sample = (duration_to_float(target)
            * self.sample_rate().get() as Float
            * self.channels().get() as Float) as u64;

        if !self.is_seekable && target_sample < self.samples_read {
            return Err(SeekError::ForwardOnly);
        }

        let samples_to_skip = if target_sample > self.samples_read
            && (self.seek_mode == SeekMode::Nearest || !self.is_seekable)
        {
            // Linearly consume samples to reach forward targets
            target_sample - self.samples_read
        } else {
            let mut reader = self
                .decoder
                .take()
                .expect("decoder should always be Some")
                .into_inner();

            let mut samples_to_skip = 0;
            if self.seek_mode == SeekMode::Nearest {
                // Rewind to start and consume samples to reach target
                reader.rewind().map_err(Arc::new)?;
                samples_to_skip = target_sample;
            } else {
                // Seek to approximate byte position
                let approximate_byte_pos = self.approx_byte_offset(target_sample);
                reader
                    .seek(SeekFrom::Start(approximate_byte_pos))
                    .map_err(Arc::new)?;

                // Clear buffer - let next() handle loading new packets
                self.current_span = None;
                self.samples_read = target_sample;
            }

            // Recreate MP3 decoder - minimp3 will handle frame synchronization
            let new_decoder = Decoder::new(reader);
            self.decoder = Some(new_decoder);

            samples_to_skip
        };

        // Consume samples to reach correct channel position
        for _ in 0..(samples_to_skip + active_channel as u64) {
            let _ = self.next();
        }

        Ok(())
    }
}

impl<R> Iterator for Mp3Decoder<R>
where
    R: Read + Seek,
{
    /// The type of samples yielded by the iterator.
    ///
    /// Returns `Sample` values representing individual audio samples.
    /// Samples are interleaved across channels in the order: channel 0, channel 1, etc.
    type Item = Sample;

    /// Returns the next audio sample from the MP3 stream.
    ///
    /// This method implements efficient frame-based decoding by maintaining the current
    /// decoded MP3 frame and returning samples one at a time. It automatically decodes
    /// new frames as needed and adapts to changing stream characteristics.
    ///
    /// # Adaptive Behavior
    ///
    /// The decoder adapts to changes in the MP3 stream:
    /// - **Bitrate tracking**: Updates average bitrate for improved seeking accuracy
    /// - **Channel changes**: Handles dynamic channel configuration changes
    /// - **Frame synchronization**: Automatically recovers from stream errors
    ///
    /// # Returns
    ///
    /// - `Some(sample)` - Next audio sample from the stream
    /// - `None` - End of stream reached or unrecoverable decoding error
    ///
    /// # Channel Order
    ///
    /// Samples are returned in interleaved order:
    /// - **Mono**: [M, M, M, ...]
    /// - **Stereo**: [L, R, L, R, ...]
    /// - **Dual Channel**: [Ch1, Ch2, Ch1, Ch2, ...]
    fn next(&mut self) -> Option<Self::Item> {
        // Hot path: return sample from current frame if available
        if let Some(current_span) = &self.current_span {
            if self.current_span_offset < current_span.data.len() {
                let v = current_span.data[self.current_span_offset];
                self.current_span_offset += 1;
                self.samples_read += 1;
                return Some(v.to_sample());
            }
        }

        // Cold path: need to decode next frame
        if let Ok(span) = self
            .decoder
            .as_mut()
            .expect("decoder should always be Some")
            .next_frame()
        {
            // Update running average bitrate with running  average (when byte_len wasn't available
            // during creation)
            let frame_bitrate_bps = span.bitrate as u32 * 1000 / 8; // Convert kbps to bytes/sec
            self.average_bitrate = ((self.average_bitrate as u64 * self.samples_read
                + frame_bitrate_bps as u64)
                / (self.samples_read + 1)) as u32;

            // Update channels if they changed (can vary between MP3 frames)
            self.channels =
                ChannelCount::new(span.channels as _).expect("mp3's have at least one channel");
            // Sample rate is fixed per MP3 stream, so no need to update
            self.current_span = Some(span);
            self.current_span_offset = 0;

            // Return first sample from the new frame
            if let Some(current_span) = &self.current_span {
                if !current_span.data.is_empty() {
                    let v = current_span.data[0];
                    self.current_span_offset = 1;
                    self.samples_read += 1;
                    return Some(v.to_sample());
                }
            }
        }

        // Stream exhausted or empty frame - set current_span to None
        self.current_span = None;
        None
    }

    /// Returns bounds on the remaining amount of samples.
    ///
    /// Provides size estimates based on MP3 metadata and current playback position.
    /// The accuracy depends on the availability and reliability of duration information.
    ///
    /// # Accuracy Levels
    ///
    /// - **High accuracy**: When total samples calculated from scanned duration
    /// - **Moderate accuracy**: When estimated from file size and average bitrate
    /// - **Conservative estimate**: When only current frame information available
    /// - **Stream exhausted**: (0, Some(0)) when no more data
    ///
    /// # Implementation
    ///
    /// The lower bound represents samples currently buffered in the decoded frame.
    /// The upper bound uses total sample estimates when available, providing useful
    /// information for progress indication and buffer allocation.
    ///
    /// # Use Cases
    ///
    /// - **Progress indication**: Upper bound enables percentage calculation
    /// - **Buffer allocation**: Lower bound ensures minimum available samples
    /// - **End detection**: (0, Some(0)) indicates stream completion
    fn size_hint(&self) -> (usize, Option<usize>) {
        // Samples already decoded and buffered (guaranteed available)
        let buffered_samples = self
            .current_span
            .as_ref()
            .map(|span| span.data.len().saturating_sub(self.current_span_offset))
            .unwrap_or(0);

        if let Some(total_samples) = self.total_samples {
            let total_remaining = total_samples.saturating_sub(self.samples_read) as usize;
            (buffered_samples, Some(total_remaining))
        } else if self.current_span.is_none() {
            // Stream exhausted
            (0, Some(0))
        } else {
            (buffered_samples, None)
        }
    }
}

/// Attempts to calculate MP3 duration using metadata headers or file scanning.
///
/// This function uses the `mp3-duration` crate to calculate duration using the most
/// efficient method available. It first searches for duration metadata in headers
/// like XING, VBRI, or INFO, and only falls back to frame-by-frame scanning if
/// no metadata is found.
///
/// # Arguments
///
/// * `data` - Mutable reference to the input stream to analyze
///
/// # Returns
///
/// - `Some(duration)` if the file was successfully analyzed
/// - `None` if scanning failed or the file is invalid
///
/// # Performance
///
/// Performance varies significantly based on available metadata:
/// - **With headers (XING/VBRI/INFO)**: Very fast, O(1) header lookup
/// - **Without headers**: Slower, O(n) frame-by-frame scanning proportional to file size
/// - **Large VBR files without headers**: Can take several seconds to analyze
///
/// # Implementation
///
/// The function:
/// 1. Saves the current stream position
/// 2. Rewinds to the beginning for analysis
/// 3. Uses `mp3-duration` crate which:
///    - First searches for XING, VBRI, or INFO headers containing duration
///    - Falls back to frame-by-frame scanning if no headers found
/// 4. Restores the original stream position
/// 5. Returns the calculated duration
///
/// # Accuracy
///
/// - **With metadata headers**: Exact duration from encoder-provided information
/// - **Frame scanning**: Sample-accurate duration from analyzing every frame
///
/// Both methods provide reliable duration information, with headers being faster.
fn get_mp3_duration<R: Read + Seek>(data: &mut R) -> Option<Duration> {
    // Save current position
    let original_pos = data.stream_position().ok()?;

    // Seek to start
    data.rewind().ok()?;

    // Try to get duration
    let duration = mp3_duration::from_read(data).ok();

    // Restore original position
    let _ = data.seek(SeekFrom::Start(original_pos));

    duration
}

/// Probes input data to detect MP3 format.
fn is_mp3<R>(data: &mut R) -> bool
where
    R: Read + Seek,
{
    utils::probe_format(data, |reader| {
        let mut decoder = Decoder::new(reader);
        decoder.next_frame().is_ok_and(|frame| {
            // Without this check, minimp3 will think it can decode WAV files. This will trigger by
            // running the test suite with features `minimp3` and (Symphonia) `wav` enabled.
            frame.bitrate != 0
        })
    })
}
