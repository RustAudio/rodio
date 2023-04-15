use crate::conversions::Sample;

use std::mem;

/// Iterator that converts from a certain sample rate to another.
#[derive(Clone, Debug)]
pub struct SampleRateConverter<I>
where
    I: Iterator,
{
    /// The iterator that gives us samples.
    input: I,
    /// We convert chunks of `from` samples into chunks of `to` samples.
    from: u32,
    /// We convert chunks of `from` samples into chunks of `to` samples.
    to: u32,
    /// Number of channels in the stream
    channels: cpal::ChannelCount,
    /// One sample per channel, extracted from `input`.
    current_frame: Vec<I::Item>,
    /// Position of `current_sample` modulo `from`.
    current_frame_pos_in_chunk: u32,
    /// The samples right after `current_sample` (one per channel), extracted from `input`.
    next_frame: Vec<I::Item>,
    /// The position of the next sample that the iterator should return, modulo `to`.
    /// This counter is incremented (modulo `to`) every time the iterator is called.
    next_output_frame_pos_in_chunk: u32,
    /// The buffer containing the samples waiting to be output.
    output_buffer: Vec<I::Item>,
}

impl<I> SampleRateConverter<I>
where
    I: Iterator,
    I::Item: Sample,
{
    ///
    ///
    /// # Panic
    ///
    /// Panics if `from` or `to` are equal to 0.
    ///
    #[inline]
    pub fn new(
        mut input: I,
        from: cpal::SampleRate,
        to: cpal::SampleRate,
        num_channels: cpal::ChannelCount,
    ) -> SampleRateConverter<I> {
        let from = match from.0 {
            0 => to.0,
            n => n,
        };
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

        let (first_samples, next_samples) = if from == to {
            // if `from` == `to` == 1, then we just pass through
            debug_assert_eq!(from, gcd);
            (Vec::new(), Vec::new())
        } else {
            let first = input
                .by_ref()
                .take(num_channels as usize)
                .collect::<Vec<_>>();
            let next = input
                .by_ref()
                .take(num_channels as usize)
                .collect::<Vec<_>>();
            (first, next)
        };

        SampleRateConverter {
            input,
            from: from / gcd,
            to: to / gcd,
            channels: num_channels,
            current_frame_pos_in_chunk: 0,
            next_output_frame_pos_in_chunk: 0,
            current_frame: first_samples,
            next_frame: next_samples,
            output_buffer: Vec::with_capacity(num_channels as usize - 1),
        }
    }

    /// Destroys this iterator and returns the underlying iterator.
    #[inline]
    pub fn into_inner(self) -> I {
        self.input
    }

    fn next_input_frame(&mut self) {
        self.current_frame_pos_in_chunk += 1;

        mem::swap(&mut self.current_frame, &mut self.next_frame);
        self.next_frame.clear();
        for _ in 0..self.channels {
            if let Some(i) = self.input.next() {
                self.next_frame.push(i);
            } else {
                break;
            }
        }
    }
}

impl<I> Iterator for SampleRateConverter<I>
where
    I: Iterator,
    I::Item: Sample + Clone,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        // the algorithm below doesn't work if `self.from == self.to`
        if self.from == self.to {
            debug_assert_eq!(self.from, 1);
            return self.input.next();
        }

        // Short circuit if there are some samples waiting.
        if !self.output_buffer.is_empty() {
            return Some(self.output_buffer.remove(0));
        }

        // The frame we are going to return from this function will be a linear interpolation
        // between `self.current_frame` and `self.next_frame`.

        if self.next_output_frame_pos_in_chunk == self.to {
            // If we jump to the next frame, we reset the whole state.
            self.next_output_frame_pos_in_chunk = 0;

            self.next_input_frame();
            while self.current_frame_pos_in_chunk != self.from {
                self.next_input_frame();
            }
            self.current_frame_pos_in_chunk = 0;
        } else {
            // Finding the position of the first sample of the linear interpolation.
            let req_left_sample =
                (self.from * self.next_output_frame_pos_in_chunk / self.to) % self.from;

            // Advancing `self.current_frame`, `self.next_frame` and
            // `self.current_frame_pos_in_chunk` until the latter variable
            // matches `req_left_sample`.
            while self.current_frame_pos_in_chunk != req_left_sample {
                self.next_input_frame();
                debug_assert!(self.current_frame_pos_in_chunk < self.from);
            }
        }

        // Merging `self.current_frame` and `self.next_frame` into `self.output_buffer`.
        // Note that `self.output_buffer` can be truncated if there is not enough data in
        // `self.next_frame`.
        let mut result = None;
        let numerator = (self.from * self.next_output_frame_pos_in_chunk) % self.to;
        for (off, (cur, next)) in self
            .current_frame
            .iter()
            .zip(self.next_frame.iter())
            .enumerate()
        {
            let sample = Sample::lerp(*cur, *next, numerator, self.to);

            if off == 0 {
                result = Some(sample);
            } else {
                self.output_buffer.push(sample);
            }
        }

        // Incrementing the counter for the next iteration.
        self.next_output_frame_pos_in_chunk += 1;

        if result.is_some() {
            result
        } else {
            // draining `self.current_frame`
            if !self.current_frame.is_empty() {
                let r = Some(self.current_frame.remove(0));
                mem::swap(&mut self.output_buffer, &mut self.current_frame);
                self.current_frame.clear();
                r
            } else {
                None
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let apply = |samples: usize| {
            // `samples_after_chunk` will contain the number of samples remaining after the chunk
            // currently being processed
            let samples_after_chunk = samples;
            // adding the samples of the next chunk that may have already been read
            let samples_after_chunk = if self.current_frame_pos_in_chunk == self.from - 1 {
                samples_after_chunk + self.next_frame.len()
            } else {
                samples_after_chunk
            };
            // removing the samples of the current chunk that have not yet been read
            let samples_after_chunk = samples_after_chunk.saturating_sub(
                self.from
                    .saturating_sub(self.current_frame_pos_in_chunk + 2) as usize
                    * usize::from(self.channels),
            );
            // calculating the number of samples after the transformation
            // TODO: this is wrong here \|/
            let samples_after_chunk = samples_after_chunk * self.to as usize / self.from as usize;

            // `samples_current_chunk` will contain the number of samples remaining to be output
            // for the chunk currently being processed
            let samples_current_chunk = (self.to - self.next_output_frame_pos_in_chunk) as usize
                * usize::from(self.channels);

            samples_current_chunk + samples_after_chunk + self.output_buffer.len()
        };

        if self.from == self.to {
            self.input.size_hint()
        } else {
            let (min, max) = self.input.size_hint();
            (apply(min), max.map(apply))
        }
    }
}

impl<I> ExactSizeIterator for SampleRateConverter<I>
where
    I: ExactSizeIterator,
    I::Item: Sample + Clone,
{
}

#[cfg(test)]
mod test {
    use super::SampleRateConverter;
    use core::time::Duration;
    use cpal::SampleRate;
    use quickcheck::quickcheck;

    // TODO: Remove once cpal 0.12.2 is released and the dependency is updated
    //  (cpal#483 implemented ops::Mul on SampleRate)
    const fn multiply_rate(r: SampleRate, k: u32) -> SampleRate {
        SampleRate(k * r.0)
    }

    quickcheck! {
        /// Check that resampling an empty input produces no output.
        fn empty(from: u32, to: u32, n: u16) -> () {
            let from = if from == 0 { return; } else { SampleRate(from) };
            let to   = if   to == 0 { return; } else { SampleRate(to)   };
            if n == 0 { return; }

            let input: Vec<u16> = Vec::new();
            let output =
                SampleRateConverter::new(input.into_iter(), from, to, n)
                  .collect::<Vec<_>>();

            assert_eq!(output, []);
        }

        /// Check that resampling to the same rate does not change the signal.
        fn identity(from: u32, n: u16, input: Vec<u16>) -> () {
            let from = if from == 0 { return; } else { SampleRate(from) };
            if n == 0 { return; }

            let output =
                SampleRateConverter::new(input.clone().into_iter(), from, from, n)
                  .collect::<Vec<_>>();

            assert_eq!(input, output);
        }

        /// Check that dividing the sample rate by k (integer) is the same as
        ///   dropping a sample from each channel.
        fn divide_sample_rate(to: u32, k: u32, input: Vec<u16>, n: u16) -> () {
            let to = if to == 0 { return; } else { SampleRate(to) };
            let from = multiply_rate(to, k);
            if k == 0 || n == 0 { return; }

            // Truncate the input, so it contains an integer number of frames.
            let input = {
                let ns = n as usize;
                let mut i = input;
                i.truncate(ns * (i.len() / ns));
                i
            };

            let output =
                SampleRateConverter::new(input.clone().into_iter(), from, to, n)
                  .collect::<Vec<_>>();

            assert_eq!(input.chunks_exact(n.into())
                         .step_by(k as usize).collect::<Vec<_>>().concat(),
                       output)
        }

        /// Check that, after multiplying the sample rate by k, every k-th
        ///  sample in the output matches exactly with the input.
        fn multiply_sample_rate(from: u32, k: u32, input: Vec<u16>, n: u16) -> () {
            let from = if from == 0 { return; } else { SampleRate(from) };
            let to = multiply_rate(from, k);
            if k == 0 || n == 0 { return; }

            // Truncate the input, so it contains an integer number of frames.
            let input = {
                let ns = n as usize;
                let mut i = input;
                i.truncate(ns * (i.len() / ns));
                i
            };

            let output =
                SampleRateConverter::new(input.clone().into_iter(), from, to, n)
                  .collect::<Vec<_>>();

            assert_eq!(input,
                       output.chunks_exact(n.into())
                         .step_by(k as usize).collect::<Vec<_>>().concat()
            )
        }

        #[ignore]
        /// Check that resampling does not change the audio duration,
        ///  except by a negligible amount (± 1ms).  Reproduces #316.
        /// Ignored, pending a bug fix.
        fn preserve_durations(d: Duration, freq: f32, to: u32) -> () {
            use crate::source::{SineWave, Source};

            let to = if to == 0 { return; } else { SampleRate(to) };
            let source = SineWave::new(freq).take_duration(d);
            let from = SampleRate(source.sample_rate());

            let resampled =
                SampleRateConverter::new(source, from, to, 1);
            let duration =
                Duration::from_secs_f32(resampled.count() as f32 / to.0 as f32);

            let delta = if d < duration { duration - d } else { d - duration };
            assert!(delta < Duration::from_millis(1),
                    "Resampled duration ({:?}) is not close to original ({:?}); Δ = {:?}",
                    duration, d, delta);
        }
    }

    #[test]
    fn upsample() {
        let input = vec![2u16, 16, 4, 18, 6, 20, 8, 22];
        let output =
            SampleRateConverter::new(input.into_iter(), SampleRate(2000), SampleRate(3000), 2);
        assert_eq!(output.len(), 12);

        let output = output.collect::<Vec<_>>();
        assert_eq!(output, [2, 16, 3, 17, 4, 18, 6, 20, 7, 21, 8, 22]);
    }
}
