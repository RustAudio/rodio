use core::time::Duration;
use std::{
    fmt::{self, Debug},
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
use symphonia::{
    core::{
        audio::{AudioSpec, GenericAudioBufferRef},
        codecs::{
            audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions},
            registry::CodecRegistry,
            CodecParameters,
        },
        errors::Error,
        formats::{probe::Hint, FormatOptions, FormatReader, SeekMode, SeekTo, SeekedTo},
        io::MediaSourceStream,
        meta::MetadataOptions,
    },
    default::get_probe,
};

use super::{DecoderError, Settings};
use crate::{
    common::{assert_error_traits, ChannelCount, Sample, SampleRate},
    source::{self, padding_samples_needed},
    Source,
};
use dasp_sample::Sample as _;

#[derive(Clone)]
pub(crate) struct Registry(Arc<RwLock<CodecRegistry>>);

impl Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Registry")
    }
}

impl Registry {
    pub(crate) fn new(registry: CodecRegistry) -> Self {
        Self(Arc::new(RwLock::new(registry)))
    }

    pub(crate) fn write(&self) -> RwLockWriteGuard<'_, CodecRegistry> {
        self.0.write().unwrap()
    }

    pub(crate) fn read(&self) -> RwLockReadGuard<'_, CodecRegistry> {
        self.0.read().unwrap()
    }
}

fn samples_from_time_f64(
    t: symphonia::core::units::Time,
    sample_rate: u32,
    channels: u32,
) -> usize {
    let (secs_i64, nanos_u32) = t.parts();
    if secs_i64 < 0 {
        return 0;
    }
    let secs = secs_i64 as f64 + (nanos_u32 as f64) / 1e9_f64;
    (secs * sample_rate as f64 * channels as f64).ceil() as usize
}

pub(crate) struct SymphoniaDecoder<'a> {
    decoder: Box<dyn AudioDecoder>,
    current_span_offset: usize,
    format: Box<dyn FormatReader + 'a>,
    total_duration: Option<Duration>,
    buffer: Vec<Sample>,
    spec: AudioSpec,
    seek_mode: SeekMode,
    selected_track_id: u32,
    samples_in_current_frame: usize,
    silence_samples_remaining: usize,
}

impl<'a> SymphoniaDecoder<'a> {
    pub(crate) fn new(
        mss: MediaSourceStream<'a>,
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
                // Catch-all for future/other Error variants (required because Error is non-exhaustive)
                _ => Err(DecoderError::IoError(format!("probe error: {:?}", e))),
            },
            Ok(Some(decoder)) => Ok(decoder),
            Ok(None) => Err(DecoderError::NoStreams),
        }
    }

    #[inline]
    pub(crate) fn into_inner(self) -> MediaSourceStream<'a> {
        self.format.into_inner()
    }

    fn init(
        mss: MediaSourceStream<'a>,
        settings: &Settings,
    ) -> symphonia::core::errors::Result<Option<SymphoniaDecoder<'a>>> {
        let mut hint = Hint::new();
        if let Some(ext) = settings.hint.as_ref() {
            hint.with_extension(ext);
        }
        if let Some(typ) = settings.mime_type.as_ref() {
            hint.mime_type(typ);
        }

        let format_opts: FormatOptions = Default::default();

        let metadata_opts: MetadataOptions = Default::default();
        let seek_mode = if settings.coarse_seek {
            SeekMode::Coarse
        } else {
            SeekMode::Accurate
        };

        // Probe the input and get the FormatReader directly.
        let mut format = get_probe().probe(&hint, mss, format_opts, metadata_opts)?;

        // Find the first track that contains audio codec parameters
        let track = format
            .tracks()
            .iter()
            .find(|t| matches!(t.codec_params.as_ref(), Some(CodecParameters::Audio(_))))
            .ok_or(symphonia::core::errors::Error::Unsupported(
                "No track with audio codec parameters",
            ))?;
        let track_id = track.id;

        let audio_params: &AudioCodecParameters = match &track.codec_params {
            Some(CodecParameters::Audio(a)) => a,
            _ => {
                return Err(symphonia::core::errors::Error::Unsupported(
                    "Track does not contain audio codec parameters",
                ))
            }
        };

        let mut decoder_opts = AudioDecoderOptions::default();
        decoder_opts.gapless = settings.gapless;

        let mut decoder = settings
            .codec_registry
            .read()
            .make_audio_decoder(audio_params, &decoder_opts)?;

        let total_duration = track
            .time_base
            .zip(track.duration)
            .and_then(|(tb, dur)| tb.calc_duration(dur))
            .and_then(|t| {
                let (secs, nanos) = t.parts();
                if secs < 0 {
                    None // std::time::Duration can't represent negative times
                } else {
                    Some(Duration::new(secs as u64, nanos))
                }
            })
            .filter(|d| !d.is_zero());

        let decoded = loop {
            let current_span = match format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => break decoder.last_decoded(), // EOF
                Err(Error::IoError(_)) => break decoder.last_decoded(),
                Err(e) => return Err(e),
            };

            // If the packet does not belong to the selected track, skip over it
            if current_span.track_id != track_id {
                continue;
            }

            match decoder.decode(&current_span) {
                Ok(decoded) if decoded.frames() > 0 => break decoded,
                Ok(_) => continue, // skip setup/header packets with no audio frames (e.g. Vorbis)
                Err(e) => match e {
                    Error::DecodeError(_) => {
                        // Decode errors are intentionally ignored with no retry limit.
                        // This behavior ensures that the decoder skips over problematic packets
                        // and continues processing the rest of the stream.
                        continue;
                    }
                    _ => return Err(e),
                },
            }
        };
        let spec = decoded.spec().to_owned();
        let buffer = SymphoniaDecoder::get_buffer(decoded);
        Ok(Some(SymphoniaDecoder {
            decoder,
            current_span_offset: 0,
            format,
            total_duration,
            buffer,
            spec,
            seek_mode,
            selected_track_id: track_id,
            samples_in_current_frame: 0,
            silence_samples_remaining: 0,
        }))
    }

    #[inline]
    fn get_buffer(decoded: GenericAudioBufferRef) -> Vec<Sample> {
        let mut out = Vec::new();
        decoded.copy_to_vec_interleaved(&mut out);
        out
    }
}

impl<'a> Source for SymphoniaDecoder<'a> {
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        Some(self.buffer.len())
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(
            self.spec
                .channels()
                .count()
                .try_into()
                .expect("rodio only support up to u16::MAX channels (65_535)"),
        )
        .expect("audio should always have at least one channel")
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(self.spec.rate()).expect("audio should always have a non zero SampleRate")
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), source::SeekError> {
        // Find track by selected_track_id
        let track = self
            .format
            .tracks()
            .iter()
            .find(|t| t.id == self.selected_track_id)
            .ok_or_else(|| {
                source::SeekError::SymphoniaDecoder(SeekError::Demuxer(Arc::new(
                    symphonia::core::errors::Error::Unsupported("Selected track not found"),
                )))
            })?;

        // Refuse accurate seek if time base is missing.
        if matches!(self.seek_mode, SeekMode::Accurate) && track.time_base.is_none() {
            return Err(source::SeekError::SymphoniaDecoder(
                SeekError::AccurateSeekNotSupported,
            ));
        }

        // Seeking should be "saturating", meaning: target positions beyond the end of the stream
        // are clamped to the end.
        let mut target = pos;
        if let Some(total_duration) = self.total_duration {
            if target > total_duration {
                target = total_duration;
            }
        }

        let tb = track.time_base.expect("time base checked above");

        let num = tb.numer.get() as u64;
        let den = tb.denom.get() as u64;

        let secs = target.as_secs();
        let nanos = target.subsec_nanos() as u64;

        // Compute integer ticks (truncated).
        let ticks_secs = secs.saturating_mul(den) / num;
        let ticks_nanos = (nanos.saturating_mul(den)) / (num * 1_000_000_000u64);
        let mut ticks = ticks_secs.saturating_add(ticks_nanos);

        // Defensive clamp: ensure ticks does not exceed the track's last valid tick (if available)
        if let Some(max_ticks_dur) = track.duration {
            let mut max_ticks: u64 = max_ticks_dur.get();
            // Make sure we use the last valid tick (avoid handing the demuxer a tick equal to duration)
            if max_ticks > 0 {
                max_ticks = max_ticks.saturating_sub(1);
            }
            if ticks > max_ticks {
                ticks = max_ticks;
            }
        }

        let units_dur = symphonia::core::units::Duration::from(ticks);
        let units_time = tb.calc_duration(units_dur).ok_or_else(|| {
            source::SeekError::SymphoniaDecoder(SeekError::AccurateSeekNotSupported)
        })?;

        // Perform seek on the format reader
        let seek_res = match self.format.seek(
            self.seek_mode,
            SeekTo::Time {
                time: units_time,
                track_id: None,
            },
        ) {
            Err(Error::SeekError(symphonia::core::errors::SeekErrorKind::ForwardOnly)) => {
                return Err(source::SeekError::SymphoniaDecoder(
                    SeekError::RandomAccessNotSupported,
                ));
            }
            other => other.map_err(Arc::new).map_err(SeekError::Demuxer),
        }?;

        // Reset decoder state and mark buffer offset invalid
        self.decoder.reset();
        self.current_span_offset = usize::MAX;

        // Refine position when accurate seek requested
        if matches!(self.seek_mode, SeekMode::Accurate) {
            self.refine_position(seek_res)?;
        }

        // After seeking, we are at the beginning of an inter-sample frame, i.e. the first
        // channel. We need to advance the iterator to the right channel.
        let active_channel = self.current_span_offset % self.channels().get() as usize;
        for _ in 0..active_channel {
            self.next();
        }

        Ok(())
    }
}

/// Error returned when the try_seek implementation of the symphonia decoder fails.
#[derive(Debug, thiserror::Error, Clone)]
pub enum SeekError {
    /// Accurate seeking is not supported
    ///
    /// This error occurs when the decoder cannot extract time base information from the source.
    /// You may catch this error to try a coarse seek instead.
    #[error("Accurate seeking is not supported on this file/byte stream that lacks time base information")]
    AccurateSeekNotSupported,
    /// The decoder does not support random access seeking
    ///
    /// This error occurs when the source is not seekable or does not have a known byte length.
    #[error("The decoder needs to know the length of the file/byte stream to be able to seek backwards. You can set that by using the `DecoderBuilder` or creating a decoder using `Decoder::try_from(some_file)`.")]
    RandomAccessNotSupported,
    /// Demuxer failed to seek
    #[error("Demuxer failed to seek")]
    Demuxer(#[source] Arc<symphonia::core::errors::Error>),
}
assert_error_traits!(SeekError);

impl<'a> SymphoniaDecoder<'a> {
    /// Note span offset must be set after
    fn refine_position(&mut self, seek_res: SeekedTo) -> Result<(), source::SeekError> {
        // Get track and time base for timestamp conversion
        let track = self
            .format
            .tracks()
            .iter()
            .find(|t| t.id == self.selected_track_id)
            .expect("selected track must exist");
        let tb = track
            .time_base
            .expect("time base availability guaranteed by caller");

        // Convert required and actual seek timestamps to `Time`
        let req_time_opt = tb.calc_time(seek_res.required_ts);
        let act_time_opt = tb.calc_time(seek_res.actual_ts);

        let sr = self.sample_rate().get();
        let ch = self.channels().get();

        // Convert times to sample counts
        let req_samples = req_time_opt
            .map(|t| samples_from_time_f64(t, sr, ch.into()))
            .unwrap_or(0);
        let act_samples = act_time_opt
            .map(|t| samples_from_time_f64(t, sr, ch.into()))
            .unwrap_or(0);

        // Compute whole-frame samples to skip
        let mut samples_to_skip = req_samples.saturating_sub(act_samples);
        samples_to_skip -= samples_to_skip % ch as usize;

        // Skip the computed number of samples via `next()`
        for _ in 0..samples_to_skip {
            self.next();
        }

        Ok(())
    }
}

impl<'a> Iterator for SymphoniaDecoder<'a> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If padding to complete a frame, return silence
            if self.silence_samples_remaining > 0 {
                self.silence_samples_remaining -= 1;
                return Some(Sample::EQUILIBRIUM);
            }

            if self.current_span_offset >= self.buffer.len() {
                // Decode next packet(s) into buffer.
                let decoded = loop {
                    let packet = match self.format.next_packet() {
                        Ok(Some(p)) if p.track_id == self.selected_track_id => p,
                        Ok(Some(_)) => continue, // packet for another track, skip
                        Ok(None) => {
                            // Input exhausted - check if mid-frame
                            let channels = self.channels();
                            self.silence_samples_remaining =
                                padding_samples_needed(self.samples_in_current_frame, channels);
                            if self.silence_samples_remaining > 0 {
                                self.samples_in_current_frame = 0;
                                break None;
                            }
                            return None;
                        }
                        Err(_) => {
                            // Error from demuxer - treat like exhaustion for padding
                            let channels = self.channels();
                            self.silence_samples_remaining =
                                padding_samples_needed(self.samples_in_current_frame, channels);
                            if self.silence_samples_remaining > 0 {
                                self.samples_in_current_frame = 0;
                                break None;
                            }
                            return None;
                        }
                    };
                    let decoded = match self.decoder.decode(&packet) {
                        Ok(decoded) => decoded,
                        Err(Error::DecodeError(_)) => {
                            // Skip over packets that cannot be decoded. This ensures the iterator
                            // continues processing subsequent packets instead of terminating due to
                            // non-critical decode errors.
                            continue;
                        }
                        Err(_) => {
                            // Input exhausted - check if mid-frame
                            let channels = self.channels();
                            self.silence_samples_remaining =
                                padding_samples_needed(self.samples_in_current_frame, channels);
                            if self.silence_samples_remaining > 0 {
                                self.samples_in_current_frame = 0;
                                break None;
                            }
                            return None;
                        }
                    };

                    // Loop until we get a packet with audio frames. This is necessary because some
                    // formats can have packets with only metadata, particularly when rewinding, in
                    // which case the iterator would otherwise end with `None`.
                    // Note: checking `decoded.frames()` is more reliable than `packet.dur()`, which
                    // can resturn non-zero durations for packets without audio frames.
                    if decoded.frames() > 0 {
                        break Some(decoded);
                    }
                };

                match decoded {
                    Some(decoded) => {
                        decoded.spec().clone_into(&mut self.spec);
                        self.buffer = SymphoniaDecoder::get_buffer(decoded);
                        self.current_span_offset = 0;
                    }
                    None => {
                        // Break out happened due to exhaustion, continue to emit padding
                        continue;
                    }
                }
            }

            let sample = *self.buffer.get(self.current_span_offset)?;
            self.current_span_offset += 1;

            let channels = self.channels();
            self.samples_in_current_frame =
                (self.samples_in_current_frame + 1) % channels.get() as usize;

            return Some(sample);
        }
    }
}
