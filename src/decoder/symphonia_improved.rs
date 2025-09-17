//! Symphonia multi-format decoder supporting AAC, FLAC, MP3, Vorbis, and more.

use std::{io::{Read, Seek, SeekFrom}, sync::Arc, time::Duration};

use symphonia::{
    core::{
        audio::SampleBuffer,
        codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL, CODEC_TYPE_VORBIS},
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
    source, Source,
};
use crate::{decoder::builder::Settings, BitDepth};

/// Adapter to use `Read + Seek` types as Symphonia `MediaSource`.
pub struct ReadSeekSource<T: Read + Seek + Send + Sync> {
    inner: T,
    byte_len: Option<u64>,
    is_seekable: bool,
}

impl<T: Read + Seek + Send + Sync> ReadSeekSource<T> {
    pub fn new(inner: T, settings: &Settings) -> Self {
        ReadSeekSource {
            inner,
            byte_len: settings.byte_len,
            is_seekable: settings.is_seekable,
        }
    }
}

impl<T: Read + Seek + Send + Sync> MediaSource for ReadSeekSource<T> {
    fn is_seekable(&self) -> bool {
        self.is_seekable
    }

    fn byte_len(&self) -> Option<u64> {
        self.byte_len
    }
}

impl<T: Read + Seek + Send + Sync> Read for ReadSeekSource<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<T: Read + Seek + Send + Sync> Seek for ReadSeekSource<T> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// Multi-format decoder using Symphonia library.
///
/// Automatically handles format detection, track selection, and codec management.
/// Supports seeking modes: fastest (coarse) vs nearest (sample-accurate).
pub struct SymphoniaDecoder {
    decoder: Box<dyn Decoder>,
    current_span_offset: usize,
    demuxer: Box<dyn FormatReader>,
    total_duration: Option<Duration>,
    sample_rate: SampleRate,
    channels: ChannelCount,
    bits_per_sample: Option<BitDepth>,
    buffer: Option<SampleBuffer<Sample>>,
    seek_mode: SeekMode,
    total_samples: Option<u64>,
    samples_read: u64,
    track_id: u32,
    is_seekable: bool,
    byte_len: Option<u64>,
}

impl SymphoniaDecoder {
    pub fn new(mss: MediaSourceStream) -> Result<Self, DecoderError> {
        Self::new_with_settings(mss, &Settings::default())
    }

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

    pub fn into_inner(self) -> MediaSourceStream {
        self.demuxer.into_inner()
    }

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

        // Find first supported track
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

        // Decode first packet to establish stream spec
        let (spec, buffer) = loop {
            let current_span = match probed.format.next_packet() {
                Ok(packet) => packet,
                Err(Error::ResetRequired) => {
                    track_id = recreate_decoder(&mut probed.format, &mut decoder, None)?;
                    continue;
                }
                Err(e) => return Err(e),
            };

            if current_span.track_id() != track_id {
                continue;
            }

            match decoder.decode(&current_span) {
                Ok(decoded) => {
                    if decoded.frames() > 0 {
                        let spec = decoded.spec().to_owned();
                        let mut sample_buffer =
                            SampleBuffer::<Sample>::new(decoded.capacity() as u64, *decoded.spec());
                        sample_buffer.copy_interleaved_ref(decoded);
                        let buffer = Some(sample_buffer);
                        break (spec, buffer);
                    }
                    continue;
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

        // Calculate total samples from metadata when available
        let total_samples = {
            if let (Some(n_frames), Some(max_frame_length)) = (
                decoder.codec_params().n_frames,
                decoder.codec_params().max_frames_per_packet,
            ) {
                n_frames.checked_mul(max_frame_length)
            } else if let Some(duration) = total_duration {
                let total_secs = duration.as_secs_f64();
                Some((total_secs * sample_rate.get() as f64 * channels.get() as f64).ceil() as u64)
            } else {
                None
            }
        };

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
        }))
    }

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
    fn current_span_len(&self) -> Option<usize> {
        self.buffer.as_ref().map(SampleBuffer::len).or(Some(0))
    }

    fn channels(&self) -> ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }

    fn bits_per_sample(&self) -> Option<BitDepth> {
        self.bits_per_sample
    }

    /// Seeks to the specified position.
    ///
    /// Behavior varies by format:
    /// - MP3 requires byte_len for coarse mode
    /// - OGG requires is_seekable flag
    /// - Backward seeks need is_seekable=true
    fn try_seek(&mut self, pos: Duration) -> Result<(), source::SeekError> {
        // Clamp to stream end
        let mut target = pos;
        if let Some(total_duration) = self.total_duration() {
            if target > total_duration {
                target = total_duration;
            }
        }

        let target_samples = (target.as_secs_f64()
            * self.sample_rate().get() as f64
            * self.channels().get() as f64) as u64;

        let active_channel = self.current_span_offset % self.channels().get() as usize;

        if !self.is_seekable {
            if target_samples < self.samples_read {
                return Err(source::SeekError::ForwardOnly);
            }

            // Linear seeking workaround for Vorbis
            if self.decoder.codec_params().codec == CODEC_TYPE_VORBIS {
                for _ in self.samples_read..target_samples {
                    let _ = self.next();
                }
                return Ok(());
            }
        }

        let seek_mode = if self.seek_mode == SeekMode::Fastest && self.byte_len.is_none() {
            SymphoniaSeekMode::Accurate // Fallback when no byte length
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

        self.decoder.reset();
        self.buffer = None;

        // Update position counter based on actual seek result
        self.samples_read = if let Some(time_base) = self.decoder.codec_params().time_base {
            let actual_time = Duration::from(time_base.calc_time(seek_res.actual_ts));
            (actual_time.as_secs_f64()
                * self.sample_rate().get() as f64
                * self.channels().get() as f64) as u64
        } else {
            seek_res.actual_ts * self.sample_rate().get() as u64 * self.channels().get() as u64
        };

        // Fine-tune to exact position for precise mode
        let mut samples_to_skip = 0;
        if self.seek_mode == SeekMode::Nearest {
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

            samples_to_skip -= samples_to_skip % self.channels().get() as usize
        };

        // Advance to correct channel position
        for _ in 0..(samples_to_skip + active_channel) {
            let _ = self.next();
        }

        Ok(())
    }
}

impl Iterator for SymphoniaDecoder {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        // Return sample from current buffer if available
        if let Some(buffer) = &self.buffer {
            if self.current_span_offset < buffer.len() {
                let sample = buffer.samples()[self.current_span_offset];
                self.current_span_offset += 1;
                self.samples_read += 1;
                return Some(sample);
            }
        }

        // Decode next packet
        let decoded = loop {
            let packet = match self.demuxer.next_packet() {
                Ok(packet) => packet,
                Err(Error::ResetRequired) => {
                    self.track_id =
                        recreate_decoder(&mut self.demuxer, &mut self.decoder, Some(self.track_id))
                            .ok()?;
                    self.cache_spec();
                    self.buffer = None;
                    continue;
                }
                Err(_) => return None,
            };

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    if decoded.frames() > 0 {
                        break decoded;
                    }
                    continue;
                }
                Err(e) => {
                    if should_continue_on_decode_error(&e, &mut self.decoder) {
                        if let Some(buffer) = self.buffer.as_mut() {
                            buffer.clear();
                        }
                        continue;
                    } else {
                        self.buffer = None;
                        return None;
                    }
                }
            }
        };

        // Update buffer with new packet
        let buffer = match self.buffer.as_mut() {
            Some(buffer) => buffer,
            None => {
                self.buffer.insert(SampleBuffer::new(
                    decoded.capacity() as u64,
                    *decoded.spec(),
                ))
            }
        };
        buffer.copy_interleaved_ref(decoded);
        self.current_span_offset = 0;

        if !buffer.is_empty() {
            let sample = buffer.samples()[0];
            self.current_span_offset = 1;
            self.samples_read += 1;
            Some(sample)
        } else {
            self.next() // Try again for metadata-only packets
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let buffered_samples = self
            .current_span_len()
            .unwrap_or(0)
            .saturating_sub(self.current_span_offset);

        if let Some(total_samples) = self.total_samples {
            let total_remaining = total_samples.saturating_sub(self.samples_read) as usize;
            (buffered_samples, Some(total_remaining))
        } else if self.buffer.is_none() {
            (0, Some(0))
        } else {
            (buffered_samples, None)
        }
    }
}

fn recreate_decoder(
    format: &mut Box<dyn FormatReader>,
    decoder: &mut Box<dyn Decoder>,
    current_track_id: Option<u32>,
) -> Result<u32, symphonia::core::errors::Error> {
    let track = if let Some(current_id) = current_track_id {
        let tracks = format.tracks();
        let current_index = tracks.iter().position(|t| t.id == current_id);

        if let Some(idx) = current_index {
            tracks
                .iter()
                .skip(idx + 1)
                .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        } else {
            None
        }
    } else {
        format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
    }
    .ok_or(Error::Unsupported(
        "No supported track found after current track",
    ))?;

    *decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    Ok(track.id)
}

fn should_continue_on_decode_error(
    error: &symphonia::core::errors::Error,
    decoder: &mut Box<dyn Decoder>,
) -> bool {
    match error {
        Error::DecodeError(_) | Error::IoError(_) => true,
        Error::ResetRequired => {
            decoder.reset();
            true
        }
        _ => false,
    }
}

impl From<SeekMode> for SymphoniaSeekMode {
    fn from(mode: SeekMode) -> Self {
        match mode {
            SeekMode::Fastest => SymphoniaSeekMode::Coarse,
            SeekMode::Nearest => SymphoniaSeekMode::Accurate,
        }
    }
}