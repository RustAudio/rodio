use core::fmt;
use core::time::Duration;
use symphonia::{
    core::{
        audio::{AudioBufferRef, SampleBuffer, SignalSpec},
        codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL},
        errors::Error,
        formats::{FormatOptions, FormatReader, SeekedTo},
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
        units::{self, Time},
    },
    default::get_probe,
};

use super::DecoderError;
use crate::common::{ChannelCount, SampleRate};
use crate::{source, Source};

// Decoder errors are not considered fatal.
// The correct action is to just get a new packet and try again.
// But a decode error in more than 3 consecutive packets is fatal.
const MAX_DECODE_RETRIES: usize = 3;

pub(crate) struct SymphoniaDecoder {
    decoder: Box<dyn Decoder>,
    current_span_offset: usize,
    format: Box<dyn FormatReader>,
    total_duration: Option<Time>,
    buffer: SampleBuffer<i16>,
    spec: SignalSpec,
}

impl SymphoniaDecoder {
    pub(crate) fn new(
        mss: MediaSourceStream,
        extension: Option<&str>,
    ) -> Result<Self, DecoderError> {
        match SymphoniaDecoder::init(mss, extension) {
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

    pub(crate) fn into_inner(self) -> MediaSourceStream {
        self.format.into_inner()
    }

    fn init(
        mss: MediaSourceStream,
        extension: Option<&str>,
    ) -> symphonia::core::errors::Result<Option<SymphoniaDecoder>> {
        let mut hint = Hint::new();
        if let Some(ext) = extension {
            hint.with_extension(ext);
        }
        let format_opts: FormatOptions = FormatOptions {
            enable_gapless: true,
            ..Default::default()
        };
        let metadata_opts: MetadataOptions = Default::default();
        let mut probed = get_probe().format(&hint, mss, &format_opts, &metadata_opts)?;

        let stream = match probed.format.default_track() {
            Some(stream) => stream,
            None => return Ok(None),
        };

        // Select the first supported track
        let track_id = probed
            .format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or(symphonia::core::errors::Error::Unsupported(
                "No track with supported codec",
            ))?
            .id;

        let track = probed
            .format
            .tracks()
            .iter()
            .find(|track| track.id == track_id)
            .unwrap();

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())?;
        let total_duration = stream
            .codec_params
            .time_base
            .zip(stream.codec_params.n_frames)
            .map(|(base, spans)| base.calc_time(spans));

        let mut decode_errors: usize = 0;
        let decoded = loop {
            let current_span = match probed.format.next_packet() {
                Ok(packet) => packet,
                Err(Error::IoError(_)) => break decoder.last_decoded(),
                Err(e) => return Err(e),
            };

            // If the packet does not belong to the selected track, skip over it
            if current_span.track_id() != track_id {
                continue;
            }

            match decoder.decode(&current_span) {
                Ok(decoded) => break decoded,
                Err(e) => match e {
                    Error::DecodeError(_) => {
                        decode_errors += 1;
                        if decode_errors > MAX_DECODE_RETRIES {
                            return Err(e);
                        } else {
                            continue;
                        }
                    }
                    _ => return Err(e),
                },
            }
        };
        let spec = decoded.spec().to_owned();
        let buffer = SymphoniaDecoder::get_buffer(decoded, &spec);
        Ok(Some(SymphoniaDecoder {
            decoder,
            current_span_offset: 0,
            format: probed.format,
            total_duration,
            buffer,
            spec,
        }))
    }

    #[inline]
    fn get_buffer(decoded: AudioBufferRef, spec: &SignalSpec) -> SampleBuffer<i16> {
        let duration = units::Duration::from(decoded.capacity() as u64);
        let mut buffer = SampleBuffer::<i16>::new(duration, *spec);
        buffer.copy_interleaved_ref(decoded);
        buffer
    }
}

impl Source for SymphoniaDecoder {
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        Some(self.buffer.samples().len())
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.spec.channels.count() as ChannelCount
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.spec.rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.total_duration.map(time_to_duration)
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), source::SeekError> {
        use symphonia::core::formats::{SeekMode, SeekTo};

        let seek_beyond_end = self
            .total_duration()
            .is_some_and(|dur| dur.saturating_sub(pos).as_millis() < 1);

        let time = if seek_beyond_end {
            let time = self.total_duration.expect("if guarantees this is Some");
            skip_back_a_tiny_bit(time) // some decoders can only seek to just before the end
        } else {
            pos.as_secs_f64().into()
        };

        // make sure the next sample is for the right channel
        let to_skip = self.current_span_offset % self.channels() as usize;

        let seek_res = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: None,
                },
            )
            .map_err(SeekError::BaseSeek)?;

        self.refine_position(seek_res)?;
        self.current_span_offset += to_skip;

        Ok(())
    }
}

/// Error returned when the try_seek implementation of the symphonia decoder fails.
#[derive(Debug)]
pub enum SeekError {
    /// Could not get next packet while refining seek position
    Refining(symphonia::core::errors::Error),
    /// Format reader failed to seek
    BaseSeek(symphonia::core::errors::Error),
    /// Decoding failed retrying on the next packet failed
    Retrying(symphonia::core::errors::Error),
    /// Decoding failed on multiple consecutive packets
    Decoding(symphonia::core::errors::Error),
}
impl fmt::Display for SeekError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeekError::Refining(err) => {
                write!(
                    f,
                    "Could not get next packet while refining seek position: {:?}",
                    err
                )
            }
            SeekError::BaseSeek(err) => {
                write!(f, "Format reader failed to seek: {:?}", err)
            }
            SeekError::Retrying(err) => {
                write!(
                    f,
                    "Decoding failed retrying on the next packet failed: {:?}",
                    err
                )
            }
            SeekError::Decoding(err) => {
                write!(
                    f,
                    "Decoding failed on multiple consecutive packets: {:?}",
                    err
                )
            }
        }
    }
}
impl std::error::Error for SeekError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SeekError::Refining(err) => Some(err),
            SeekError::BaseSeek(err) => Some(err),
            SeekError::Retrying(err) => Some(err),
            SeekError::Decoding(err) => Some(err),
        }
    }
}

impl SymphoniaDecoder {
    /// Note span offset must be set after
    fn refine_position(&mut self, seek_res: SeekedTo) -> Result<(), source::SeekError> {
        let mut samples_to_pass = seek_res.required_ts - seek_res.actual_ts;
        let packet = loop {
            let candidate = self.format.next_packet().map_err(SeekError::Refining)?;
            if candidate.dur() > samples_to_pass {
                break candidate;
            } else {
                samples_to_pass -= candidate.dur();
            }
        };

        let mut decoded = self.decoder.decode(&packet);
        for _ in 0..MAX_DECODE_RETRIES {
            if decoded.is_err() {
                let packet = self.format.next_packet().map_err(SeekError::Retrying)?;
                decoded = self.decoder.decode(&packet);
            }
        }

        let decoded = decoded.map_err(SeekError::Decoding)?;
        decoded.spec().clone_into(&mut self.spec);
        self.buffer = SymphoniaDecoder::get_buffer(decoded, &self.spec);
        self.current_span_offset = samples_to_pass as usize * self.channels() as usize;
        Ok(())
    }
}

fn skip_back_a_tiny_bit(
    Time {
        mut seconds,
        mut frac,
    }: Time,
) -> Time {
    frac -= 0.0001;
    if frac < 0.0 {
        seconds = seconds.saturating_sub(1);
        frac = 1.0 - frac;
    }
    Time { seconds, frac }
}

fn time_to_duration(time: Time) -> Duration {
    Duration::new(
        time.seconds,
        if time.frac > 0.0 {
            (1f64 / time.frac) as u32
        } else {
            0
        },
    )
}

impl Iterator for SymphoniaDecoder {
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        if self.current_span_offset >= self.buffer.len() {
            let packet = self.format.next_packet().ok()?;
            let mut decoded = self.decoder.decode(&packet);
            for _ in 0..MAX_DECODE_RETRIES {
                if decoded.is_err() {
                    let packet = self.format.next_packet().ok()?;
                    decoded = self.decoder.decode(&packet);
                }
            }
            let decoded = decoded.ok()?;
            decoded.spec().clone_into(&mut self.spec);
            self.buffer = SymphoniaDecoder::get_buffer(decoded, &self.spec);
            self.current_span_offset = 0;
        }

        let sample = *self.buffer.samples().get(self.current_span_offset)?;
        self.current_span_offset += 1;

        Some(sample)
    }
}
