use std::io::Read;
use super::Decoder;

use cpal::{self, Voice};
use hound::WavReader;
use hound::WavSpec;

pub struct WavDecoder<R> where R: Read {
    reader: WavReader<R>,
    spec: WavSpec,
}

impl<R> WavDecoder<R> where R: Read {
    pub fn new(data: R) -> Result<WavDecoder<R>, ()> {
        let reader = match WavReader::new(data) {
            Err(_) => return Err(()),
            Ok(r) => r
        };

        let spec = reader.spec();

        Ok(WavDecoder {
            reader: reader,
            spec: spec,
        })
    }
}

impl<R> Decoder for WavDecoder<R> where R: Read {
    fn write(&mut self, voice: &mut Voice) {
        let mut samples = self.reader.samples::<i16>();
        let samples_left = samples.len();
        if samples_left == 0 { return; }

        // TODO: hack because of a bug in cpal
        let samples_left = if samples_left > 512 { 512 } else { samples_left };

        let mut buffer: cpal::Buffer<u16> =
            voice.append_data(self.spec.channels,
                              cpal::SamplesRate(self.spec.sample_rate),
                              samples_left);

        for (dest, src) in buffer.iter_mut().zip(&mut samples) {
            // TODO: There is a bug in cpal that handles signed samples in the
            // wrong manner, so we cast it to `u16` for now.
            *dest = src.unwrap() as u16;
        }
    }
}
