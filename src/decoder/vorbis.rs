use std::io::{Read, Seek};
use std::f64::INFINITY;
use super::Decoder;
use conversions;

use cpal::{self, Endpoint, Voice};
use vorbis;

pub struct VorbisDecoder {
    reader: conversions::AmplifierIterator<Box<Iterator<Item=i16> + Send>>,
    voice: Voice,
}

impl VorbisDecoder {
    pub fn new<R>(endpoint: &Endpoint, data: R) -> Result<VorbisDecoder, ()>
                  where R: Read + Seek + Send + 'static
    {
        let decoder = match vorbis::Decoder::new(data) {
            Err(_) => return Err(()),
            Ok(r) => r
        };

        // building the voice
        let voice_format = endpoint.get_supported_formats_list().unwrap().next().unwrap();
        let voice = Voice::new(endpoint, &voice_format).unwrap();

        let to_channels = voice.get_channels();
        let to_samples_rate = voice.get_samples_rate();

        let reader = decoder.into_packets().filter_map(|p| p.ok()).flat_map(move |packet| {
            let reader = packet.data.into_iter();
            let reader = conversions::ChannelsCountConverter::new(reader, packet.channels,
                                                                  to_channels);
            let reader = conversions::SamplesRateConverter::new(reader, cpal::SamplesRate(packet.rate as u32),
                                                                to_samples_rate, to_channels);
            reader
        });

        Ok(VorbisDecoder {
            reader: conversions::AmplifierIterator::new(Box::new(reader), 1.0),
            voice: voice,
        })
    }
}

impl Decoder for VorbisDecoder {
    fn write(&mut self) -> bool {
        // TODO: handle end

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
        unimplemented!()
    }

    fn get_remaining_duration_ms(&self) -> u32 {
        unimplemented!()
    }
}
