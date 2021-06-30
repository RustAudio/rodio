use std::time::Duration;
use symphonia::{
    core::{
        audio::{AudioBufferRef, SampleBuffer, SignalSpec},
        codecs::{Decoder, DecoderOptions},
        errors::Error,
        formats::{FormatOptions, FormatReader},
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
        units,
    },
    default::get_probe,
};

use crate::Source;

use super::DecoderError;

pub struct SymphoniaDecoder {
    decoder: Box<dyn Decoder>,
    current_frame_offset: usize,
    format: Box<dyn FormatReader>,
    buffer: SampleBuffer<i16>,
    spec: SignalSpec,
}

impl SymphoniaDecoder {
    pub fn new(mss: MediaSourceStream, extension: Option<&str>) -> Result<Self, DecoderError> {
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

    pub fn into_inner(self: Box<Self>) -> MediaSourceStream {
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
        let format_opts: FormatOptions = Default::default();
        let metadata_opts: MetadataOptions = Default::default();
        let mut probed = get_probe().format(&hint, mss, &format_opts, &metadata_opts)?;

        let stream = match probed.format.default_track() {
            Some(stream) => stream,
            None => return Ok(None),
        };

        let mut decoder = symphonia::default::get_codecs().make(
            &stream.codec_params,
            &DecoderOptions {
                verify: true,
                ..Default::default()
            },
        )?;

        let current_frame = probed.format.next_packet()?;
        let decoded = decoder.decode(&current_frame)?;
        let spec = decoded.spec().to_owned();
        let buffer = SymphoniaDecoder::get_buffer(decoded, &spec);

        return Ok(Some(SymphoniaDecoder {
            decoder,
            current_frame_offset: 0,
            format: probed.format,
            buffer,
            spec,
        }));
    }

    #[inline]
    fn get_buffer(decoded: AudioBufferRef, spec: &SignalSpec) -> SampleBuffer<i16> {
        let duration = units::Duration::from(decoded.capacity() as u64);
        let mut buffer = SampleBuffer::<i16>::new(duration, spec.clone());
        buffer.copy_interleaved_ref(decoded);
        return buffer;
    }
}

impl Source for SymphoniaDecoder {
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.buffer.samples().len())
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.spec.channels.count() as u16
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.spec.rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Iterator for SymphoniaDecoder {
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        if self.current_frame_offset == self.buffer.len() {
            match self.format.next_packet() {
                Ok(packet) => match self.decoder.decode(&packet) {
                    Ok(decoded) => {
                        self.spec = decoded.spec().to_owned();
                        self.buffer = SymphoniaDecoder::get_buffer(decoded, &self.spec);
                    }
                    Err(_) => return None,
                },
                Err(_) => return None,
            }
            self.current_frame_offset = 0;
        }

        let sample = self.buffer.samples()[self.current_frame_offset];
        self.current_frame_offset += 1;

        Some(sample)
    }
}
