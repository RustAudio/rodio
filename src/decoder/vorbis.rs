use std::io::{Read, Seek};
use std::mem;
use super::Decoder;
use super::conversions;

use cpal::{self, Voice};
use vorbis;

pub struct VorbisDecoder<R> where R: Read + Seek {
    decoder: vorbis::Decoder<R>,
    current_packet: Option<vorbis::Packet>,
}

impl<R> VorbisDecoder<R> where R: Read + Seek {
    pub fn new(data: R) -> Result<VorbisDecoder<R>, ()> {
        let decoder = match vorbis::Decoder::new(data) {
            Err(_) => return Err(()),
            Ok(r) => r
        };

        Ok(VorbisDecoder {
            decoder: decoder,
            current_packet: None,
        })
    }
}

impl<R> Decoder for VorbisDecoder<R> where R: Read + Seek {
    fn write(&mut self, voice: &mut Voice) {
        // setting the current packet to `None` if there is no data left in it
        match &mut self.current_packet {
            packet @ &mut Some(_) => {
                if packet.as_ref().unwrap().data.len() == 0 {
                    *packet = None;
                }
            },
            _ => ()
        };

        // getting the next packet
        let packet = if let Some(ref mut packet) = self.current_packet {
            packet
        } else {
            let next = match self.decoder.packets().next().and_then(|r| r.ok()) {
                Some(p) => p,
                None => return,     // TODO: handle
            };

            self.current_packet = Some(next);
            self.current_packet.as_mut().unwrap()
        };

        let to_channels = voice.get_channels();
        let to_samples_rate = voice.get_samples_rate();

        let mut buffer = voice.append_data(packet.data.len());
        let src = mem::replace(&mut packet.data, Vec::new());

        conversions::convert_and_write(&src, packet.channels, to_channels,
                                       cpal::SamplesRate(packet.rate as u32), to_samples_rate,
                                       &mut buffer);

        /*
        let mut src = src.into_iter();
        for (dest, src) in buffer.iter_mut().zip(src.by_ref()) {
            *dest = src;
        }
        packet.data = src.collect();*/
    }
}
