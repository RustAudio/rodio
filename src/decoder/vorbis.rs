use std::io::{Read, Seek};
use std::time::Duration;
use std::vec;

use Source;

use vorbis;

/// Decoder for an OGG file that contains Vorbis sound format.
pub struct VorbisDecoder<R> where R: Read + Seek {
    decoder: vorbis::Decoder<R>,
    current_data: vec::IntoIter<i16>,
    current_samples_rate: u32,
    current_channels: u16,
}

impl<R> VorbisDecoder<R> where R: Read + Seek {
    /// Attempts to decode the data as ogg/vorbis.
    pub fn new(data: R) -> Result<VorbisDecoder<R>, ()> {
        let mut decoder = match vorbis::Decoder::new(data) {
            Err(_) => return Err(()),
            Ok(r) => r
        };

        let (data, rate, channels) = match decoder.packets().filter_map(Result::ok).next() {
            Some(p) => (p.data, p.rate as u32, p.channels as u16),
            None => (Vec::new(), 44100, 2),
        };

        Ok(VorbisDecoder {
            decoder: decoder,
            current_data: data.into_iter(),
            current_samples_rate: rate,
            current_channels: channels,
        })
    }
}

impl<R> Source for VorbisDecoder<R> where R: Read + Seek {
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        Some(self.current_data.len())
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        self.current_channels
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        self.current_samples_rate
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        None
    }
}

impl<R> Iterator for VorbisDecoder<R> where R: Read + Seek {
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        // TODO: do better
        if let Some(sample) = self.current_data.next() {
            if self.current_data.len() == 0 {
                if let Some(packet) = self.decoder.packets().filter_map(Result::ok).next() {
                    self.current_data = packet.data.into_iter();
                    self.current_samples_rate = packet.rate as u32;
                    self.current_channels = packet.channels;
                }
            }

            return Some(sample);
        }

        if let Some(packet) = self.decoder.packets().filter_map(Result::ok).next() {
            self.current_data = packet.data.into_iter();
            self.current_samples_rate = packet.rate as u32;
            self.current_channels = packet.channels;
            Some(self.current_data.next().unwrap())

        } else {
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.current_data.size_hint().0, None)
    }
}
