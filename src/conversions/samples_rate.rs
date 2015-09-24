use cpal;
use conversions::Sample;

use std::iter;
use std::mem;

/// Iterator that converts from a certain samples rate to another.
pub struct SamplesRateConverter<I> where I: Iterator {
    /// The iterator that gives us samples.
    input: I,
    /// We convert chunks of `from` samples into chunks of `to` samples.
    from: u32,
    /// We convert chunks of `from` samples into chunks of `to` samples.
    to: u32,
    /// One sample per channel, extracted from `input`.
    current_samples: Vec<I::Item>,
    /// Position of `current_sample` modulo `from`.
    current_sample_pos_in_chunk: u32,
    /// The samples right after `current_sample` (one per channel), extracted from `input`.
    next_samples: Vec<I::Item>,
    /// The position of the next sample that the iterator should return, modulo `to`.
    /// This counter is incremented (modulo `to`) every time the iterator is called.
    next_output_sample_pos_in_chunk: u32,
    /// The buffer containing the samples waiting to be output.
    output_buffer: Vec<I::Item>,
}

impl<I> SamplesRateConverter<I> where I: Iterator, I::Item: Sample {
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

        let samples = if from == to {
            // if `from` == `to` == 1, then we just pass through
            debug_assert_eq!(from, gcd);
            Vec::new()
        } else {
            input.by_ref().take(num_channels as usize).collect()
        };

        SamplesRateConverter {
            input: input,
            from: from / gcd,
            to: to / gcd,
            current_sample_pos_in_chunk: 0,
            next_output_sample_pos_in_chunk: 0,
            current_samples: Vec::with_capacity(num_channels as usize),
            next_samples: samples,
            output_buffer: Vec::with_capacity(num_channels as usize - 1),
        }
    }
}

impl<I> Iterator for SamplesRateConverter<I> where I: Iterator, I::Item: Sample + Clone {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        // the algorithm below doesn't work if `self.from == self.to`
        if self.from == self.to {
            debug_assert_eq!(self.from, 1);
            return self.input.next();
        }

        if self.output_buffer.len() >= 1 {
            return Some(self.output_buffer.remove(0));
        }

        // The sample we are going to return from this function will be a linear interpolation
        // between `self.current_sample` and `self.next_sample`.

        // Finding the position of the first sample of the linear interpolation.
        let req_left_sample = (self.from * self.next_output_sample_pos_in_chunk / self.to) %
                              self.from;

        // Advancing `self.current_sample`, `self.next_sample` and
        // `self.current_sample_pos_in_chunk` until the latter variable matches `req_left_sample`.
        // We also always advance one step if `next_output_sample_pos_in_chunk` equals 0.
        let mut advancing_required = self.next_output_sample_pos_in_chunk == 0;
        while advancing_required || self.current_sample_pos_in_chunk != req_left_sample {
            advancing_required = false;
            self.current_sample_pos_in_chunk += 1;
            self.current_sample_pos_in_chunk %= self.from;

            if self.current_samples.len() >= 1 ||
               self.current_sample_pos_in_chunk == req_left_sample
            {
                mem::swap(&mut self.current_samples, &mut self.next_samples);
                self.next_samples.clear();
                for _ in (0 .. self.next_samples.capacity()) {
                    if let Some(i) = self.input.next() {
                        self.next_samples.push(i);
                    } else {
                        break;
                    }
                }
            }
        }

        // Merging `self.current_samples` and `self.next_samples` into `self.output_buffer`.
        let mut result = None;
        let numerator = (self.from * self.next_output_sample_pos_in_chunk) % self.to;
        for (off, (cur, next)) in self.current_samples.iter().zip(self.next_samples.iter()).enumerate() {
            let sample = Sample::lerp(cur.clone(), next.clone(), numerator, self.to);

            if off == 0 {
                result = Some(sample);
            } else {
                self.output_buffer.push(sample);
            }
        }

        // Incrementing the counter for the next iteration.
        self.next_output_sample_pos_in_chunk += 1;
        self.next_output_sample_pos_in_chunk %= self.to;

        if result.is_some() {
            result
        } else {
            // draining `self.current_samples`
            if self.current_samples.len() >= 1 {
                let r = Some(self.current_samples.remove(0));
                mem::swap(&mut self.output_buffer, &mut self.current_samples);
                self.current_samples.clear();
                r
            } else {
                None
            }
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
    use cpal::SamplesRate;
    use super::SamplesRateConverter;

    #[test]
    fn zero() {
        let input: Vec<u16> = Vec::new();
        let output = SamplesRateConverter::new(input.into_iter(), SamplesRate(1278),
                                               SamplesRate(78923), 1).collect::<Vec<_>>();

        assert_eq!(output.len(), 0);
        assert_eq!(output, []);
    }

    #[test]
    fn identity_1channel() {
        let input = vec![2u16, 16, 4, 18, 6, 20, 8, 22];
        let output = SamplesRateConverter::new(input.into_iter(), SamplesRate(12345),
                                               SamplesRate(12345), 1).collect::<Vec<_>>();

        assert_eq!(output.len(), 8);
        assert_eq!(output, [2u16, 16, 4, 18, 6, 20, 8, 22]);
    }

    #[test]
    fn identity_2channels() {
        let input = vec![2u16, 16, 4, 18, 6, 20, 8, 22];
        let output = SamplesRateConverter::new(input.into_iter(), SamplesRate(12345),
                                               SamplesRate(12345), 2).collect::<Vec<_>>();

        assert_eq!(output.len(), 8);
        assert_eq!(output, [2u16, 16, 4, 18, 6, 20, 8, 22]);
    }

    #[test]
    fn identity_2channels_misalign() {
        let input = vec![2u16, 16, 4, 18, 6];
        let output = SamplesRateConverter::new(input.into_iter(), SamplesRate(12345),
                                               SamplesRate(12345), 2).collect::<Vec<_>>();
        assert_eq!(output.len(), 5);
        assert_eq!(output, [2u16, 16, 4, 18, 6]);
    }

    #[test]
    fn identity_5channels() {
        let input = vec![2u16, 16, 4, 18, 6, 20, 8, 22, 10, 24];
        let output = SamplesRateConverter::new(input.into_iter(), SamplesRate(12345),
                                               SamplesRate(12345), 5).collect::<Vec<_>>();
        assert_eq!(output.len(), 10);
        assert_eq!(output, [2u16, 16, 4, 18, 6, 20, 8, 22, 10, 24]);
    }

    #[test]
    fn half_samples_rate() {
        let input = vec![1u16, 16, 2, 17, 3, 18, 4, 19, 5, 20, 6, 21];
        let output = SamplesRateConverter::new(input.into_iter(), SamplesRate(44100),
                                               SamplesRate(22050), 2).collect::<Vec<_>>();

        assert_eq!(output.len(), 6);
        assert_eq!(output, [1, 16, 3, 18, 5, 20]);
    }

    #[test]
    fn double_samples_rate() {
        let input = vec![2u16, 16, 4, 18, 6, 20, 8, 22];
        let output = SamplesRateConverter::new(input.into_iter(), SamplesRate(22050),
                                               SamplesRate(44100), 2).collect::<Vec<_>>();

        assert_eq!(output.len(), 14);
        assert_eq!(output, [2, 16, 3, 17, 4, 18, 5, 19, 6, 20, 7, 21, 8, 22]);
    }

    #[test]
    fn downsample() {
        let input = vec![2u16, 16, 4, 18, 6, 20, 8, 22];
        let output = SamplesRateConverter::new(input.into_iter(), SamplesRate(2000),
                                               SamplesRate(3000), 2).collect::<Vec<_>>();

        assert_eq!(output.len(), 12);
        assert_eq!(output, [2, 16, 3, 17, 4, 18, 6, 20, 7, 21, 8, 22]);
    }
}
