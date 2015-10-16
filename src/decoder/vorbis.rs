use std::io::{Read, Seek};

use Source;

//use vorbis;

pub struct VorbisDecoder {
    reader: Box<Iterator<Item=f32> + Send>,
}

impl VorbisDecoder {
    pub fn new<R>(_data: R) -> Result<VorbisDecoder, ()>
                  where R: Read + Seek + Send + 'static
    {
        /*let decoder = match vorbis::Decoder::new(data) {
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
            reader: Box::new(reader),
        })*/

        unimplemented!()
    }
}

impl Source for VorbisDecoder {
    #[inline]
    fn get_current_frame_len(&self) -> usize {
        self.len()
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        unimplemented!()
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
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
