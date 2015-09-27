use std::io::{Read, Seek, SeekFrom};
use std::cmp;
use super::Decoder;
use conversions;

use cpal::{self, Endpoint, Voice};
use hound::WavReader;

pub struct WavDecoder {
    reader: conversions::AmplifierIterator<Box<Iterator<Item=i16> + Send>>,
    voice: Voice,
    total_duration_ms: u32,
}

impl WavDecoder {
    pub fn new<R>(endpoint: &Endpoint, mut data: R) -> Result<WavDecoder, R>
                  where R: Read + Seek + Send + 'static
    {
        if !is_wave(data.by_ref()) {
            return Err(data);
        }

        let reader = WavReader::new(data).unwrap();
        let spec = reader.spec();
        let total_duration_ms = reader.duration() * 1000 / spec.sample_rate;

        // choosing a format amongst the ones available
        let voice_format = endpoint.get_supported_formats_list().unwrap().fold(None, |f1, f2| {
            if f1.is_none() {
                return Some(f2);
            }

            let f1 = f1.unwrap();

            if f1.samples_rate.0 % spec.sample_rate == 0 {
                return Some(f1);
            }

            if f2.samples_rate.0 % spec.sample_rate == 0 {
                return Some(f2);
            }

            if f1.channels.len() >= spec.channels as usize {
                return Some(f1);
            }

            if f2.channels.len() >= spec.channels as usize {
                return Some(f2);
            }

            if f1.data_type == cpal::SampleFormat::I16 {
                return Some(f1);
            }

            if f2.data_type == cpal::SampleFormat::I16 {
                return Some(f2);
            }

            Some(f1)
        }).unwrap();

        let voice = Voice::new(endpoint, &voice_format).unwrap();

        let reader = SamplesIterator { reader: reader, samples_read: 0 };
        let reader = conversions::ChannelsCountConverter::new(reader, spec.channels,
                                                              voice.get_channels());
        let reader = conversions::SamplesRateConverter::new(reader, cpal::SamplesRate(spec.sample_rate),
                                                            voice.get_samples_rate(), voice.get_channels());

        Ok(WavDecoder {
            reader: conversions::AmplifierIterator::new(Box::new(reader), 1.0),
            voice: voice,
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
    fn write(&mut self) -> bool {
        if let (0, _) = self.reader.size_hint() {
            return false;
        }

        {
            let samples = self.voice.get_samples_rate().0 * self.voice.get_channels() as u32;
            let mut buffer = self.voice.append_data(samples as usize);
            conversions::convert_and_write(self.reader.by_ref(), &mut buffer);
        }

        self.voice.play();
        true
    }

    fn set_volume(&mut self, value: f32) {
        self.reader.set_amplification(value);
    }

    fn get_total_duration_ms(&self) -> u32 {
        self.total_duration_ms
    }

    fn get_remaining_duration_ms(&self) -> u32 {
        let (num_samples, _) = self.reader.size_hint();
        let num_samples = num_samples + self.voice.get_pending_samples();

        (num_samples as u64 * 1000 /
                (self.voice.get_samples_rate().0 as u64 * self.voice.get_channels() as u64)) as u32
    }
}
