use crate::conversions::Sample;

use num_rational::Ratio;
use std::mem;
use std::time::Duration;
use crate::Source;

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
    to_full: u32,
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
    /// Create new sample rate converter.
    ///
    /// The converter uses simple linear interpolation for up-sampling
    /// and discards samples for down-sampling. This may introduce audible
    /// distortions in some cases (see [#584](https://github.com/RustAudio/rodio/issues/584)).
    ///
    /// # Limitations
    /// Some rate conversions where target rate is high and rates are mutual primes the sample
    /// interpolation may cause numeric overflows. Conversion between usual sample rates
    /// 2400, 8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, ... is expected to work.
    ///
    /// # Panic
    /// Panics if `from`, `to` or `num_channels` are 0.
    #[inline]
    pub fn new(
        mut input: I,
        from: cpal::SampleRate,
        to: cpal::SampleRate,
        num_channels: cpal::ChannelCount,
    ) -> SampleRateConverter<I> {
        let from = from.0;
        let to = to.0;

        assert!(num_channels >= 1);
        assert!(from >= 1);
        assert!(to >= 1);
        let to_full = to;

        let (first_samples, next_samples) = if from == to {
            // if `from` == `to` == 1, then we just pass through
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

        // Reducing numerator to avoid numeric overflows during interpolation.
        let (to, from) = Ratio::new(to, from).into_raw();

        SampleRateConverter {
            input,
            from,
            to,
            to_full,
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

    /// get mutable access to the iterator
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.input
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

impl<I> Source for SampleRateConverter<I>
where
    I: Source,
    I::Item: Sample,
{
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.input.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.to_full
    }

    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
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
    use cpal::{ChannelCount, SampleRate};
    use quickcheck::{quickcheck, TestResult};

    quickcheck! {
        /// Check that resampling an empty input produces no output.
        fn empty(from: u16, to: u16, channels: u8) -> TestResult {
            if channels == 0 || channels > 128
                || from == 0
                || to == 0
            {
                return TestResult::discard();
            }
            let from = SampleRate(from as u32);
            let to   = SampleRate(to as u32);

            let input: Vec<u16> = Vec::new();
            let output =
                SampleRateConverter::new(input.into_iter(), from, to, channels as ChannelCount)
                  .collect::<Vec<_>>();

            assert_eq!(output, []);
            TestResult::passed()
        }

        /// Check that resampling to the same rate does not change the signal.
        fn identity(from: u16, channels: u8, input: Vec<u16>) -> TestResult {
            if channels == 0 || channels > 128 || from == 0 { return TestResult::discard(); }
            let from = SampleRate(from as u32);

            let output =
                SampleRateConverter::new(input.clone().into_iter(), from, from, channels as ChannelCount)
                  .collect::<Vec<_>>();

            TestResult::from_bool(input == output)
        }

        /// Check that dividing the sample rate by k (integer) is the same as
        ///   dropping a sample from each channel.
        fn divide_sample_rate(to: u16, k: u16, input: Vec<u16>, channels: u8) -> TestResult {
            if k == 0 || channels == 0 || channels > 128 || to == 0 || to > 48000 {
                return TestResult::discard();
            }

            let to = SampleRate(to as u32);
            let from = to * k as u32;

            // Truncate the input, so it contains an integer number of frames.
            let input = {
                let ns = channels as usize;
                let mut i = input;
                i.truncate(ns * (i.len() / ns));
                i
            };

            let output =
                SampleRateConverter::new(input.clone().into_iter(), from, to, channels as ChannelCount)
                  .collect::<Vec<_>>();

            TestResult::from_bool(input.chunks_exact(channels.into())
                         .step_by(k as usize).collect::<Vec<_>>().concat() == output)
        }

        /// Check that, after multiplying the sample rate by k, every k-th
        ///  sample in the output matches exactly with the input.
        fn multiply_sample_rate(from: u16, k: u8, input: Vec<u16>, channels: u8) -> TestResult {
            if k == 0 || channels == 0 || channels > 128 || from == 0 {
                return TestResult::discard();
            }

            let from = SampleRate(from as u32);
            let to = from * k as u32;

            // Truncate the input, so it contains an integer number of frames.
            let input = {
                let ns = channels as usize;
                let mut i = input;
                i.truncate(ns * (i.len() / ns));
                i
            };

            let output =
                SampleRateConverter::new(input.clone().into_iter(), from, to, channels as ChannelCount)
                  .collect::<Vec<_>>();

            TestResult::from_bool(input ==
                       output.chunks_exact(channels.into())
                         .step_by(k as usize).collect::<Vec<_>>().concat())
        }

        #[ignore]
        /// Check that resampling does not change the audio duration,
        ///  except by a negligible amount (± 1ms).  Reproduces #316.
        /// Ignored, pending a bug fix.
        fn preserve_durations(d: Duration, freq: f32, to: u32) -> TestResult {
            if to == 0 { return TestResult::discard(); }

            use crate::source::{SineWave, Source};

            let to = SampleRate(to);
            let source = SineWave::new(freq).take_duration(d);
            let from = SampleRate(source.sample_rate());

            let resampled =
                SampleRateConverter::new(source, from, to, 1);
            let duration =
                Duration::from_secs_f32(resampled.count() as f32 / to.0 as f32);

            let delta = if d < duration { duration - d } else { d - duration };
            TestResult::from_bool(delta < Duration::from_millis(1))
        }
    }

    #[test]
    fn upsample() {
        let input = vec![2u16, 16, 4, 18, 6, 20, 8, 22];
        let output =
            SampleRateConverter::new(input.into_iter(), SampleRate(2000), SampleRate(3000), 2);
        assert_eq!(output.len(), 12); // Test the source's Iterator::size_hint()

        let output = output.collect::<Vec<_>>();
        assert_eq!(output, [2, 16, 3, 17, 4, 18, 6, 20, 7, 21, 8, 22]);
    }

    #[test]
    fn upsample2() {
        let input = vec![1u16, 14];
        let output =
            SampleRateConverter::new(input.into_iter(), SampleRate(1000), SampleRate(7000), 1);
        let size_estimation = output.len();
        let output = output.collect::<Vec<_>>();
        assert_eq!(output, [1, 2, 4, 6, 8, 10, 12, 14]);
        assert!((size_estimation as f32 / output.len() as f32).abs() < 2.0);
    }

    #[test]
    fn downsample() {
        let input = Vec::from_iter(0u16..17);
        let output =
            SampleRateConverter::new(input.into_iter(), SampleRate(12000), SampleRate(2400), 1);
        let size_estimation = output.len();
        let output = output.collect::<Vec<_>>();
        assert_eq!(output, [0, 5, 10, 15]);
        assert!((size_estimation as f32 / output.len() as f32).abs() < 2.0);
    }
}
