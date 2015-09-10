/*!
This module contains function that will convert from one PCM format to another.

This includes conversion between samples formats, channels or sample rates.

*/
use std::borrow::Cow;
use std::cmp;
use std::mem;
use std::iter;

use cpal;
use cpal::UnknownTypeBuffer;
use cpal::SampleFormat;

///
pub fn convert_and_write<I, S>(mut samples: I, output: &mut UnknownTypeBuffer)
                               where I: Iterator<Item=S>, S: Sample
{
    match output {
        &mut UnknownTypeBuffer::U16(ref mut buffer) => {
            for (i, o) in samples.zip(buffer.iter_mut()) {
                *o = i.to_u16();
            }
        },

        &mut UnknownTypeBuffer::I16(ref mut buffer) => {
            for (i, o) in samples.zip(buffer.iter_mut()) {
                *o = i.to_i16();
            }
        },

        &mut UnknownTypeBuffer::F32(ref mut buffer) => {
            for (i, o) in samples.zip(buffer.iter_mut()) {
                *o = i.to_f32();
            }
        },
    }
}

/// Trait for containers that contain PCM data.
pub trait Sample: cpal::Sample {
    /// Returns the average inside a list.
    fn average(data: &[Self]) -> Self;

    fn to_i16(&self) -> i16;
    fn to_u16(&self) -> u16;
    fn to_f32(&self) -> f32;
}

impl Sample for u16 {
    #[inline]
    fn average(data: &[u16]) -> u16 {
        let sum: usize = data.iter().fold(0, |acc, &item| acc + item as usize);
        (sum / data.len()) as u16
    }

    #[inline]
    fn to_i16(&self) -> i16 {
        if *self >= 32768 {
            (*self - 32768) as i16
        } else {
            (*self as i16) - 32767 - 1
        }
    }

    #[inline]
    fn to_u16(&self) -> u16 {
        *self
    }

    #[inline]
    fn to_f32(&self) -> f32 {
        self.to_i16().to_f32()
    }
}

impl Sample for i16 {
    #[inline]
    fn average(data: &[i16]) -> i16 {
        let sum: isize = data.iter().fold(0, |acc, &item| acc + item as isize);
        (sum / data.len() as isize) as i16
    }

    #[inline]
    fn to_i16(&self) -> i16 {
        *self
    }

    #[inline]
    fn to_u16(&self) -> u16 {
        if *self < 0 {
            (*self - ::std::i16::MIN) as u16
        } else {
            (*self as u16) + 32768
        }
    }

    #[inline]
    fn to_f32(&self) -> f32 {
        if *self < 0 {
            *self as f32 / -(::std::i16::MIN as f32)
        } else {
            *self as f32 / ::std::i16::MAX as f32
        }
    }
}

impl Sample for f32 {
    #[inline]
    fn average(data: &[f32]) -> f32 {
        let sum: f64 = data.iter().fold(0.0, |acc, &item| acc + item as f64);
        (sum / data.len() as f64) as f32
    }

    #[inline]
    fn to_i16(&self) -> i16 {
        if *self >= 0.0 {
            (*self * ::std::i16::MAX as f32) as i16
        } else {
            (-*self * ::std::i16::MIN as f32) as i16
        }
    }

    #[inline]
    fn to_u16(&self) -> u16 {
        (((*self + 1.0) * 0.5) * ::std::u16::MAX as f32).round() as u16
    }

    #[inline]
    fn to_f32(&self) -> f32 {
        *self
    }
}

/// Iterator that converts from a certain channels count to another.
pub struct ChannelsCountConverter<I> where I: Iterator {
    input: I,
    from: cpal::ChannelsCount,
    to: cpal::ChannelsCount,
    output_buffer: Vec<I::Item>,
}

impl<I> ChannelsCountConverter<I> where I: Iterator {
    ///
    ///
    /// # Panic
    ///
    /// Panicks if `from` or `to` are equal to 0.
    ///
    pub fn new(input: I, from: cpal::ChannelsCount, to: cpal::ChannelsCount)
               -> ChannelsCountConverter<I>
    {
        assert!(from >= 1);
        assert!(to >= 1);

        ChannelsCountConverter {
            input: input,
            from: from,
            to: to,
            output_buffer: Vec::with_capacity(to as usize),
        }
    }
}

impl<I> Iterator for ChannelsCountConverter<I> where I: Iterator, I::Item: Clone {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        if self.output_buffer.len() == 0 {
            // copying common channels from input to output
            for _ in (0 .. cmp::min(self.from, self.to)) {
                self.output_buffer.push(match self.input.next() {
                    Some(i) => i,
                    None => return None
                });
            }

            // adding extra output channels
            // TODO: could be done better
            if self.to > self.from {
                for _ in (0 .. self.to - self.from) {
                    let val = self.output_buffer[0].clone();
                    self.output_buffer.push(val);
                }
            }

            // discarding extra channels
            if self.from > self.to {
                for _ in (0 .. self.from - self.to) {
                    let _ = self.input.next();
                }
            }
        }

        Some(self.output_buffer.remove(0))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (min, max) = self.input.size_hint();

        let min = (min / self.from as usize) * self.to as usize + self.output_buffer.len();
        let max = max.map(|max| (max / self.from as usize) * self.to as usize + self.output_buffer.len());

        (min, max)
    }
}

impl<I> ExactSizeIterator for ChannelsCountConverter<I>
                              where I: ExactSizeIterator, I::Item: Clone {}

/// Iterator that converts from a certain samples rate to another.
pub struct SamplesRateConverter<I> where I: Iterator {
    input: I,
    from: u32,
    to: u32,
    output_buffer: Vec<I::Item>,
}

impl<I> SamplesRateConverter<I> where I: Iterator {
    ///
    ///
    /// # Panic
    ///
    /// Panicks if `from` or `to` are equal to 0.
    ///
    pub fn new(input: I, from: cpal::SamplesRate, to: cpal::SamplesRate)
               -> SamplesRateConverter<I>
    {
        let from = from.0;
        let to = to.0;

        assert!(from >= 1);
        assert!(to >= 1);

        // finding greatest common divisor
        // TODO: better method
        let gcd = {
            let mut value = cmp::min(from, to);
            while (from % value) != 0 || (to % value) != 0 {
                value -= 1;
            }
            value
        };

        SamplesRateConverter {
            input: input,
            from: from / gcd,
            to: to / gcd,
            output_buffer: Vec::with_capacity(to as usize),
        }
    }
}

impl<I> Iterator for SamplesRateConverter<I> where I: Iterator, I::Item: Sample + Clone {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        if self.output_buffer.len() == 0 {
            if self.input.size_hint().1.unwrap() == 0 {
                return None;
            }

            // reading samples from the input
            let input = self.input.by_ref().take(self.from as usize);

            // and duplicating each sample `to` times
            let self_to = self.to as usize;
            let input = input.flat_map(|val| iter::repeat(val).take(self_to));
            let input: Vec<_> = input.collect();
            // the length of `input` is `from * to`

            // now taking chunks of `from` size and building the average of each chunk
            // therefore the remaining list is of size `to`
            self.output_buffer = input.chunks(self.from as usize)
                                      .map(|chunk| Sample::average(chunk)).collect();
        }

        Some(self.output_buffer.remove(0))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (min, max) = self.input.size_hint();

        let min = (min / self.from as usize) * self.to as usize + self.output_buffer.len();
        let max = max.map(|max| (max / self.from as usize) * self.to as usize + self.output_buffer.len());

        (min, max)
    }
}

impl<I> ExactSizeIterator for SamplesRateConverter<I>
                              where I: ExactSizeIterator, I::Item: Sample + Clone {}

#[cfg(test)]
mod test {
    use super::Sample;
    /*#[test]
    fn remove_channels() {
        let result = convert_channels(&[1u16, 2, 3, 1, 2, 3], 3, 2);
        assert_eq!(result, [1, 2, 1, 2]);

        let result = convert_channels(&[1u16, 2, 3, 4, 1, 2, 3, 4], 4, 1);
        assert_eq!(result, [1, 1]);
    }

    #[test]
    fn add_channels() {
        let result = convert_channels(&[1u16, 2, 1, 2], 2, 3);
        assert_eq!(result, [1, 2, 1, 1, 2, 1]);

        let result = convert_channels(&[1u16, 2, 1, 2], 2, 4);
        assert_eq!(result, [1, 2, 1, 2, 1, 2, 1, 2]);
    }

    #[test]
    #[should_panic]
    fn convert_channels_wrong_data_len() {
        convert_channels(&[1u16, 2, 3], 2, 1);
    }

    #[test]
    fn half_samples_rate() {
        let result = convert_samples_rate(&[1u16, 16, 2, 17, 3, 18, 4, 19],
                                          ::SamplesRate(44100), ::SamplesRate(22050), 2);

        assert_eq!(result, [1, 16, 3, 18]);
    }

    #[test]
    fn double_samples_rate() {
        let result = convert_samples_rate(&[2u16, 16, 4, 18, 6, 20, 8, 22],
                                          ::SamplesRate(22050), ::SamplesRate(44100), 2);

        assert_eq!(result, [2, 16, 3, 17, 4, 18, 5, 19, 6, 20, 7, 21, 8, 22]);
    }*/

    #[test]
    fn i16_to_i16() {
        assert_eq!(0i16.to_i16(), 0);
        assert_eq!((-467i16).to_i16(), -467);
        assert_eq!(32767i16.to_i16(), 32767);
        assert_eq!((-32768i16).to_i16(), -32768);
    }

    #[test]
    fn i16_to_u16() {
        assert_eq!(0i16.to_u16(), 32768);
        assert_eq!((-16384i16).to_u16(), 16384);
        assert_eq!(32767i16.to_u16(), 65535);
        assert_eq!((-32768i16).to_u16(), 0);
    }

    #[test]
    fn i16_to_f32() {
        assert_eq!(0i16.to_f32(), 0.0);
        assert_eq!((-16384i16).to_f32(), -0.5);
        assert_eq!(32767i16.to_f32(), 1.0);
        assert_eq!((-32768i16).to_f32(), -1.0);
    }

    #[test]
    fn u16_to_i16() {
        assert_eq!(32768u16.to_i16(), 0);
        assert_eq!(16384u16.to_i16(), -16384);
        assert_eq!(65535u16.to_i16(), 32767);
        assert_eq!(0u16.to_i16(), -32768);
    }

    #[test]
    fn u16_to_u16() {
        assert_eq!(0u16.to_u16(), 0);
        assert_eq!(467u16.to_u16(), 467);
        assert_eq!(32767u16.to_u16(), 32767);
        assert_eq!(65535u16.to_u16(), 65535);
    }

    #[test]
    fn u16_to_f32() {
        assert_eq!(0u16.to_f32(), -1.0);
        assert_eq!(32768u16.to_f32(), 0.0);
        assert_eq!(65535u16.to_f32(), 1.0);
    }

    #[test]
    fn f32_to_i16() {
        assert_eq!(0.0f32.to_i16(), 0);
        assert_eq!((-0.5f32).to_i16(), ::std::i16::MIN / 2);
        assert_eq!(1.0f32.to_i16(), ::std::i16::MAX);
        assert_eq!((-1.0f32).to_i16(), ::std::i16::MIN);
    }

    #[test]
    fn f32_to_u16() {
        assert_eq!((-1.0f32).to_u16(), 0);
        assert_eq!(0.0f32.to_u16(), 32768);
        assert_eq!(1.0f32.to_u16(), 65535);
    }

    #[test]
    fn f32_to_f32() {
        assert_eq!(0.1f32.to_f32(), 0.1);
        assert_eq!((-0.7f32).to_f32(), -0.7);
        assert_eq!(1.0f32.to_f32(), 1.0);
    }
}
