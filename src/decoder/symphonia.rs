use std::time::Duration;

use symphonia::core::{
    audio::SampleBuffer,
    codecs::{Decoder, DecoderOptions},
    formats::{FormatOptions, FormatReader, Packet},
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
    units,
};

use crate::Source;

pub struct SymphoniaDecoder {
    decoder: Box<dyn Decoder>,
    current_frame: Packet,
    current_frame_offset: usize,
    format: Box<dyn FormatReader>,
    buffer: SampleBuffer<i16>,
    channels: usize,
}

unsafe impl Send for SymphoniaDecoder {}

impl SymphoniaDecoder {
    pub fn new(mss: MediaSourceStream, extension: Option<&str>) -> Result<Self, ()> {
        let mut hint = Hint::new();
        if let Some(ext) = extension {
            hint.with_extension(ext);
        }
        let format_opts: FormatOptions = Default::default();
        let metadata_opts: MetadataOptions = Default::default();
        let mut probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &metadata_opts)
            .unwrap();

        let stream = probed.format.default_stream().unwrap();

        let mut decoder = symphonia::default::get_codecs()
            .make(
                &stream.codec_params,
                &DecoderOptions {
                    verify: true,
                    ..Default::default()
                },
            )
            .unwrap();

        let current_frame = probed.format.next_packet().unwrap();

        let decoded = decoder.decode(&current_frame).unwrap();
        let spec = decoded.spec().clone();
        let duration = symphonia::core::units::Duration::from(decoded.capacity() as u64);
        let mut buf = SampleBuffer::<i16>::new(duration, spec.to_owned());
        buf.copy_interleaved_ref(decoded);

        return Ok(SymphoniaDecoder {
            decoder,
            current_frame,
            current_frame_offset: 0,
            format: probed.format,
            buffer: buf,
            channels: spec.channels.count(),
        });
    }
    pub fn into_inner(self: Box<Self>) -> MediaSourceStream {
        self.format.into_inner()
    }
}

impl Source for SymphoniaDecoder {
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.buffer.samples().len())
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.channels as u16
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.format
            .default_stream()
            .unwrap()
            .codec_params
            .sample_rate
            .unwrap()
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
                Ok(packet) => {
                    self.current_frame = packet;

                    match self.decoder.decode(&self.current_frame) {
                        Ok(decoded) => {
                            let spec = decoded.spec();
                            let duration = units::Duration::from(decoded.capacity() as u64);
                            let mut buf = SampleBuffer::<i16>::new(duration, spec.to_owned());
                            buf.copy_interleaved_ref(decoded);
                            self.buffer = buf;
                        }
                        Err(_) => return None,
                    }
                }
                Err(_) => return None,
            }
            self.current_frame_offset = 0;
        }

        let sample = self.buffer.samples()[self.current_frame_offset];

        self.current_frame_offset += 1;

        Some(sample)
    }
}
