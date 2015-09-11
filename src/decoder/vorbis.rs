use std::io::{Read, Seek};
use std::mem;
use super::Decoder;
use super::conversions;

use cpal::{self, Endpoint, Voice};
use vorbis;

pub struct VorbisDecoder {
    reader: Box<Iterator<Item=i16> + Send>,
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
                                                                to_samples_rate);
            reader
        });

        Ok(VorbisDecoder {
            reader: Box::new(reader),
            voice: voice,
        })
    }
}

impl Decoder for VorbisDecoder {
    fn write(&mut self) -> u64 {
        /*let (min, _) = self.reader.size_hint();

        if min == 0 {
            // finished
            return;
        }*/

        let len = {
            let mut buffer = self.voice.append_data(32768);
            let len = buffer.len();
            conversions::convert_and_write(self.reader.by_ref(), &mut buffer);
            len
        };

        self.voice.play();

        len as u64 * 1000000000 / self.voice.get_samples_rate().0 as u64
    }
}
