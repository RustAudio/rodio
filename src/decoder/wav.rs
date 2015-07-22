use std::io::{Read, Seek, SeekFrom};
use super::Decoder;

use cpal::{self, Voice};
use hound::WavReader;
use hound::WavSpec;

pub struct WavDecoder<R> where R: Read {
    reader: WavReader<R>,
    spec: WavSpec,
}

impl<R> WavDecoder<R> where R: Read + Seek {
    pub fn new(mut data: R) -> Result<WavDecoder<R>, R> {
        let stream_pos = data.seek(SeekFrom::Current(0)).unwrap();

        if WavReader::new(data.by_ref()).is_err() {
            data.seek(SeekFrom::Start(stream_pos)).unwrap();
            return Err(data);
        }

        data.seek(SeekFrom::Start(stream_pos)).unwrap();
        let reader = WavReader::new(data).unwrap();
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
