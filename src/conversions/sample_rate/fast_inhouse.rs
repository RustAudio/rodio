use crate::conversions::Sample;

use num_rational::Ratio;
use std::marker::PhantomData;
use std::mem;

#[cfg(test)]
mod test;

/// Iterator that converts from a certain sample rate to another.
#[derive(Clone, Debug)]
pub struct SampleRateConverter<I, O>
where
    I: Iterator,
    O: Sample,
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

    output_type: PhantomData<O>,
}

impl<I, O> SampleRateConverter<I, O>
where
    I: Iterator,
    I::Item: Sample,
    O: Sample,
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
    ) -> SampleRateConverter<I, O> {
        let from = from.0;
        let to = to.0;

        assert!(num_channels >= 1);
        assert!(from >= 1);
        assert!(to >= 1);

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
            channels: num_channels,
            current_frame_pos_in_chunk: 0,
            next_output_frame_pos_in_chunk: 0,
            current_frame: first_samples,
            next_frame: next_samples,
            output_buffer: Vec::with_capacity(num_channels as usize - 1),
            output_type: PhantomData,
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

impl<I, O> Iterator for SampleRateConverter<I, O>
where
    I: Iterator,
    I::Item: Sample + Clone,
    O: Sample + cpal::FromSample<I::Item>,
{
    type Item = O;

    fn next(&mut self) -> Option<O> {
        // the algorithm below doesn't work if `self.from == self.to`
        if self.from == self.to {
            debug_assert_eq!(self.from, 1);
            return self.input.next().map(|s| cpal::Sample::from_sample(s));
        }

        // Short circuit if there are some samples waiting.
        if !self.output_buffer.is_empty() {
            return Some(self.output_buffer.remove(0)).map(|s| cpal::Sample::from_sample(s));
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
            result.map(|s| cpal::Sample::from_sample(s))
        } else {
            // draining `self.current_frame`
            if !self.current_frame.is_empty() {
                let r = Some(self.current_frame.remove(0));
                mem::swap(&mut self.output_buffer, &mut self.current_frame);
                self.current_frame.clear();
                r.map(|s| cpal::Sample::from_sample(s))
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

impl<I, O> ExactSizeIterator for SampleRateConverter<I, O>
where
    I: ExactSizeIterator,
    I::Item: Sample + Clone,
    O: Sample + cpal::FromSample<I::Item>,
{
}
