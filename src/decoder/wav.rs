use std::io::{Read, Seek, SeekFrom};
use std::cmp;
use super::Decoder;
use conversions;

use cpal::{self, Endpoint, Voice};
use hound::WavReader;

pub struct WavDecoder {
    reader: conversions::AmplifierIterator<Box<Iterator<Item=f32> + Send>>,
    total_duration_ms: u32,
}

impl WavDecoder {
    pub fn new<R>(mut data: R, output_channels: u16, output_samples_rate: u32)
                  -> Result<WavDecoder, R>
                  where R: Read + Seek + Send + 'static
    {
        if !is_wave(data.by_ref()) {
            return Err(data);
        }

        let reader = WavReader::new(data).unwrap();
        let spec = reader.spec();
        let total_duration_ms = reader.duration() * 1000 / spec.sample_rate;

        let reader = SamplesIterator { reader: reader, samples_read: 0 };
        let reader = conversions::ChannelsCountConverter::new(reader, spec.channels, 2);
        let reader = conversions::SamplesRateConverter::new(reader, cpal::SamplesRate(spec.sample_rate),
                                                            cpal::SamplesRate(output_samples_rate), output_channels);
        let reader = conversions::DataConverter::new(reader);

        Ok(WavDecoder {
            reader: conversions::AmplifierIterator::new(Box::new(reader), 1.0),
            total_duration_ms: total_duration_ms,
        })
    }
}

struct SamplesIterator<R> where R: Read + Seek {
    reader: WavReader<R>,
    samples_read: u32,
}

impl<R> Iterator for SamplesIterator<R> where R: Read + Seek {
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        if let Some(value) = self.reader.samples().next() {
            self.samples_read += 1;
            Some(value.unwrap_or(0))
        } else {
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = (self.reader.len() - self.samples_read) as usize;
        (len, Some(len))
    }
}

impl<R> ExactSizeIterator for SamplesIterator<R> where R: Read + Seek {}

/// Returns true if the stream contains WAV data, then resets it to where it was.
fn is_wave<R>(mut data: R) -> bool where R: Read + Seek {
    let stream_pos = data.seek(SeekFrom::Current(0)).unwrap();

    if WavReader::new(data.by_ref()).is_err() {
        data.seek(SeekFrom::Start(stream_pos)).unwrap();
        return false;
    }

    data.seek(SeekFrom::Start(stream_pos)).unwrap();
    true
}

impl Decoder for WavDecoder {
    fn set_volume(&mut self, value: f32) {
        self.reader.set_amplification(value);
    }

    fn get_total_duration_ms(&self) -> u32 {
        self.total_duration_ms
    }
}

impl Iterator for WavDecoder {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        self.reader.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.reader.size_hint()
    }
}

impl ExactSizeIterator for WavDecoder {}
