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
    #[inline]
    pub fn new(mut input: I, from: cpal::SamplesRate, to: cpal::SamplesRate)
               -> SamplesRateConverter<I>
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
