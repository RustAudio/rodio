use std::io::{Read, Seek};
use std::f64::INFINITY;
use super::Decoder;
use conversions;

use cpal::{self, Endpoint, Voice};
use vorbis;

pub struct VorbisDecoder {
    reader: conversions::AmplifierIterator<Box<Iterator<Item=f32> + Send>>,
}

impl VorbisDecoder {
    pub fn new<R>(data: R, output_channels: u16, output_samples_rate: u32)
                  -> Result<VorbisDecoder, ()>
                  where R: Read + Seek + Send + 'static
    {
        let decoder = match vorbis::Decoder::new(data) {
            Err(_) => return Err(()),
            Ok(r) => r
        };

        let reader = decoder.into_packets().filter_map(|p| p.ok()).flat_map(move |packet| {
            let reader = packet.data.into_iter();
            let reader = conversions::ChannelsCountConverter::new(reader, packet.channels,
                                                                  output_channels);
            let reader = conversions::SamplesRateConverter::new(reader, cpal::SamplesRate(packet.rate as u32),
                                                                cpal::SamplesRate(output_samples_rate), output_channels);
            let reader = conversions::DataConverter::new(reader);
            reader
        });

        Ok(VorbisDecoder {
            reader: conversions::AmplifierIterator::new(Box::new(reader), 1.0),
        })
    }
}

impl Decoder for VorbisDecoder {
    fn set_volume(&mut self, value: f32) {
        self.reader.set_amplification(value);
    }

    fn get_total_duration_ms(&self) -> u32 {
        unimplemented!()
    }
}

impl Iterator for VorbisDecoder {
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

impl ExactSizeIterator for VorbisDecoder {}
