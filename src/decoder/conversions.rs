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
    let samples = samples.chain(iter::repeat(Sample::zero_value()));

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
    fn lerp(first: Self, second: Self, numerator: u32, denominator: u32) -> Self;

    fn zero_value() -> Self;

    fn to_i16(&self) -> i16;
    fn to_u16(&self) -> u16;
    fn to_f32(&self) -> f32;
}

impl Sample for u16 {
    #[inline]
    fn lerp(first: u16, second: u16, numerator: u32, denominator: u32) -> u16 {
        (first as u32 + (second as u32 - first as u32) * numerator / denominator) as u16
    }

    #[inline]
    fn zero_value() -> u16 {
        32768
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
    fn lerp(first: i16, second: i16, numerator: u32, denominator: u32) -> i16 {
        (first as i32 + (second as i32 - first as i32) * numerator as i32 / denominator as i32) as i16
    }

    #[inline]
    fn zero_value() -> i16 {
        0
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
    fn lerp(first: f32, second: f32, numerator: u32, denominator: u32) -> f32 {
        first + (second - first) * numerator as f32 / denominator as f32
    }

    #[inline]
    fn zero_value() -> f32 {
        0.0
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
                for i in (0 .. self.to - self.from) {
                    let val = self.output_buffer[(i % self.from) as usize].clone();
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
    /// The iterator that gives us samples.
    input: I,
    /// We convert chunks of `from` samples into chunks of `to` samples.
    from: u32,
    /// We convert chunks of `from` samples into chunks of `to` samples.
    to: u32,
    /// One sample extracted from `input`.
    current_sample: Option<I::Item>,
    /// Position of `current_sample` modulo `from`.
    current_sample_pos_in_chunk: u32,
    /// The sample right after `current_sample`, extracted from `input`.
    next_sample: Option<I::Item>,
    /// The position of the next sample that the iterator should return, modulo `to`.
    /// This counter is incremented (modulo `to`) every time the iterator is called.
    next_output_sample_pos_in_chunk: u32,
}

impl<I> SamplesRateConverter<I> where I: Iterator {
    ///
    ///
    /// # Panic
    ///
    /// Panicks if `from` or `to` are equal to 0.
    ///
    pub fn new(mut input: I, from: cpal::SamplesRate, to: cpal::SamplesRate)
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

        let first_sample = input.next();
        let second_sample = input.next();

        SamplesRateConverter {
            input: input,
            from: from / gcd,
            to: to / gcd,
            current_sample_pos_in_chunk: 0,
            next_output_sample_pos_in_chunk: 0,
            current_sample: first_sample,
            next_sample: second_sample,
        }
    }
}

impl<I> Iterator for SamplesRateConverter<I> where I: Iterator, I::Item: Sample + Clone {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        // The sample we are going to return from this function will be a linear interpolation
        // between `self.current_sample` and `self.next_sample`.

        // Finding the position of the first sample of the linear interpolation.
        let req_left_sample = (self.from * self.next_output_sample_pos_in_chunk / self.to) %
                              self.from;

        // Advancing `self.current_sample`, `self.next_sample` and
        // `self.current_sample_pos_in_chunk` until the latter variable matches `req_left_sample`.
        while self.current_sample_pos_in_chunk != req_left_sample {
            self.current_sample_pos_in_chunk += 1;
            self.current_sample_pos_in_chunk %= self.from;
            self.current_sample = self.next_sample;
            self.next_sample = self.input.next();
        }

        // Doing the linear interpolation. We handle a possible end of stream here.
        let result = match (self.current_sample, self.next_sample) {
            (Some(ref cur), Some(ref next)) => {
                let numerator = (self.from * self.next_output_sample_pos_in_chunk) % self.to;
                Sample::lerp(cur.clone(), next.clone(), numerator, self.to)
            },

            (Some(ref cur), None) if self.next_output_sample_pos_in_chunk == 0 => {
                cur.clone()
            },

            _ => return None,
        };

        // Incrementing the counter for the next iteration.
        self.next_output_sample_pos_in_chunk += 1;
        self.next_output_sample_pos_in_chunk %= self.to;

        Some(result)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (min, max) = self.input.size_hint();

        // TODO: inexact?
        let min = (min / self.from as usize) * self.to as usize;
        let max = max.map(|max| (max / self.from as usize) * self.to as usize);

        (min, max)
    }
}

impl<I> ExactSizeIterator for SamplesRateConverter<I>
                              where I: ExactSizeIterator, I::Item: Sample + Clone {}

#[cfg(test)]
mod test {
    use super::Sample;
    use super::ChannelsCountConverter;

    #[test]
    fn remove_channels() {
        let input = vec![1u16, 2, 3, 1, 2, 3];
        let output = ChannelsCountConverter::new(input.into_iter(), 3, 2).collect::<Vec<_>>();
        assert_eq!(output, [1, 2, 1, 2]);

        let input = vec![1u16, 2, 3, 4, 1, 2, 3, 4];
        let output = ChannelsCountConverter::new(input.into_iter(), 4, 1).collect::<Vec<_>>();
        assert_eq!(output, [1, 1]);
    }

    #[test]
    fn add_channels() {
        let input = vec![1u16, 2, 1, 2];
        let output = ChannelsCountConverter::new(input.into_iter(), 2, 3).collect::<Vec<_>>();
        assert_eq!(output, [1, 2, 1, 1, 2, 1]);

        let input = vec![1u16, 2, 1, 2];
        let output = ChannelsCountConverter::new(input.into_iter(), 2, 4).collect::<Vec<_>>();
        assert_eq!(output, [1, 2, 1, 2, 1, 2, 1, 2]);
    }

    /*
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
