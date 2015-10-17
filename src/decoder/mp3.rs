use std::io::{Read, Seek/*, SeekFrom*/};
use std::time::Duration;

use Source;

use simplemad;

pub struct Mp3Decoder<R> where R: Read + Send + 'static {
    reader: simplemad::Decoder<R>,
    current_frame: simplemad::Frame,
    current_frame_channel: usize,
    current_frame_sample_pos: usize,
}

impl<R> Mp3Decoder<R> where R: Read + Seek + Send + 'static {
    pub fn new(data: R) -> Result<Mp3Decoder<R>, ()> {
        let mut reader = match simplemad::Decoder::decode(data) {
            Ok(r) => r,
            Err(_) => return Err(())
        };

        let current_frame = next_frame(&mut reader);

        Ok(Mp3Decoder {
            reader: reader,
            current_frame: current_frame,
            current_frame_channel: 0,
            current_frame_sample_pos: 0,
        })
    }
}

impl<R> Source for Mp3Decoder<R> where R: Read + Send + 'static {
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        Some(self.current_frame.samples[0].len())
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        self.current_frame.samples.len() as u16
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        self.current_frame.sample_rate
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        None        // TODO: not supported
    }
}

impl<R> Iterator for Mp3Decoder<R> where R: Read + Send + 'static {
    type Item = i16;        // TODO: i32

    #[inline]
    fn next(&mut self) -> Option<i16> {        // TODO: i32
        if self.current_frame.samples[0].len() == 0 {
            return None;
        }

        let sample = (self.current_frame.samples[self.current_frame_channel][self.current_frame_sample_pos] / 0x10000) as i16;
        self.current_frame_channel += 1;

        if self.current_frame_channel < self.current_frame.samples.len() {
            return Some(sample);
        }

        self.current_frame_channel = 0;
        self.current_frame_sample_pos += 1;

        if self.current_frame_sample_pos < self.current_frame.samples[0].len() {
            return Some(sample);
        }
        
        self.current_frame = next_frame(&mut self.reader);
        self.current_frame_channel = 0;
        self.current_frame_sample_pos = 0;

        return Some(sample);
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.current_frame.samples[0].len(), None)
    }
}

/// Returns the next frame of a decoder, ignores errors.
fn next_frame<R>(decoder: &mut simplemad::Decoder<R>) -> simplemad::Frame
                 where R: Read + Send + 'static
{
    let frame = decoder.filter_map(|f| f.ok()).next();
    let frame = frame.unwrap_or_else(|| {
        simplemad::Frame {
            sample_rate: 44100,
            samples: vec![Vec::new()],
            position: 0.0,
            duration: 0.0,
        }
    });

    frame
}
