use std::io::{Read, Seek, SeekFrom};
use super::Decoder;

use conversions::ChannelsCountConverter;
use conversions::SamplesRateConverter;
use conversions::DataConverter;

use cpal;
use hound::WavReader;

pub struct WavDecoder<R> where R: Read + Seek {
    reader: DataConverter<SamplesRateConverter<ChannelsCountConverter<SamplesIterator<R>>>, f32>,
    total_duration_ms: u32,
}

impl<R> WavDecoder<R> where R: Read + Seek {
    pub fn new(mut data: R, output_channels: u16, output_samples_rate: u32)
               -> Result<WavDecoder<R>, R>
    {
        if !is_wave(data.by_ref()) {
            return Err(data);
        }

        let reader = WavReader::new(data).unwrap();
        let spec = reader.spec();
        let total_duration_ms = reader.duration() * 1000 / spec.sample_rate;

        let reader = SamplesIterator { reader: reader, samples_read: 0 };
        let reader = ChannelsCountConverter::new(reader, spec.channels, output_channels);
        let reader = SamplesRateConverter::new(reader, cpal::SamplesRate(spec.sample_rate),
                                               cpal::SamplesRate(output_samples_rate), output_channels);
        let reader = DataConverter::new(reader);

        Ok(WavDecoder {
            reader: reader,
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

impl<R> Decoder for WavDecoder<R> where R: Read + Seek {
    fn get_total_duration_ms(&self) -> u32 {
        self.total_duration_ms
    }
}

impl<R> Iterator for WavDecoder<R> where R: Read + Seek {
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

impl<R> ExactSizeIterator for WavDecoder<R> where R: Read + Seek {}
