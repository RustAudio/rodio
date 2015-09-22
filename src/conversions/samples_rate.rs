use std::mem;

use arrayvec::ArrayVec;

use cpal;
use conversions::Sample;

/// Iterator that converts from a certain samples rate to another.
pub struct SamplesRateConverter<I> where I: Iterator {
    /// The iterator that gives us samples.
    input: I,
    /// We convert chunks of `from` samples into chunks of `to` samples.
    from: u32,
    /// We convert chunks of `from` samples into chunks of `to` samples.
    to: u32,
    /// One sample per channel, extracted from `input`.
    current_samples: ArrayVec<[I::Item; 12]>,
    /// Position of `current_sample` modulo `from`.
    current_sample_pos_in_chunk: u32,
    /// The samples right after `current_sample` (one per channel), extracted from `input`.
    next_samples: ArrayVec<[I::Item; 12]>,
    /// The position of the next sample that the iterator should return, modulo `to`.
    /// This counter is incremented (modulo `to`) every time the iterator is called.
    next_output_sample_pos_in_chunk: u32,
    /// The buffer containing the samples waiting to be output.
    output_buffer: ArrayVec<[I::Item; 12]>,
}

impl<I> SamplesRateConverter<I> where I: Iterator {
    ///
    ///
    /// # Panic
    ///
    /// Panicks if `from` or `to` are equal to 0.
    ///
    #[inline]
    pub fn new(mut input: I, from: cpal::SamplesRate, to: cpal::SamplesRate,
               num_channels: cpal::ChannelsCount) -> SamplesRateConverter<I>
    {
        let from = from.0;
        let to = to.0;

        assert!(from >= 1);
        assert!(to >= 1);

        // finding greatest common divisor
        let gcd = {
            #[inline]
            fn gcd(a: u32, b: u32) -> u32 {
                if b == 0 {
                    a
                } else {
                    gcd(b, a % b)
                }
            }

            gcd(from, to)
        };

        let first_samples = input.by_ref().take(num_channels as usize).collect();
        let second_samples = input.by_ref().take(num_channels as usize).collect();

        SamplesRateConverter {
            input: input,
            from: from / gcd,
            to: to / gcd,
            current_sample_pos_in_chunk: 0,
            next_output_sample_pos_in_chunk: 0,
            current_samples: first_samples,
            next_samples: second_samples,
            output_buffer: ArrayVec::new(),   // with_capacity(num_channels as usize)
        }
    }
}

impl<I> Iterator for SamplesRateConverter<I> where I: Iterator, I::Item: Sample + Clone {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        if self.output_buffer.len() >= 1 {
            // note: ArrayVec doesn't mimic the Vec API for remove
            return self.output_buffer.remove(0);
        }

        if self.current_samples.len() == 0 {
            return None;
        }

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

            mem::swap(&mut self.current_samples, &mut self.next_samples);
            for (o, i) in self.next_samples.iter_mut().zip(self.input.by_ref()) { *o = i; }
        }

        self.output_buffer = self.current_samples.iter().zip(self.next_samples.iter())
                                                 .map(|(cur, next)|
        {
            let numerator = (self.from * self.next_output_sample_pos_in_chunk) % self.to;
            Sample::lerp(cur.clone(), next.clone(), numerator, self.to)
        }).collect();

        // Incrementing the counter for the next iteration.
        self.next_output_sample_pos_in_chunk += 1;
        self.next_output_sample_pos_in_chunk %= self.to;

        if self.output_buffer.len() >= 1 {
            // note: ArrayVec doesn't mimic the Vec API for remove
            self.output_buffer.remove(0)
        } else {
            None
        }
    }

    #[inline]
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
    /*use super::SamplesRateConverter;

    
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
}
