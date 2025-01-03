// high fidelity resampler using the rubato crate. In contradiction to the
// fast in house resampler this does not provide ExactSizeIterator. We cannot
// do that since rubato does not guaranteed the amount of samples returned

use rubato::{Resampler, SincInterpolationParameters};

use crate::Sample;

#[cfg(test)]
mod test;

/// Rubato requires the samples for each channel to be in separate buffers.
/// This wrapper around Vec<Vec<f64>> provides an iterator that returns
/// samples interleaved.
struct ResamplerOutput {
    channel_buffers: Vec<Vec<f64>>,
    frames_in_buffer: usize,
    next_channel: usize,
    next_frame: usize,
}

impl ResamplerOutput {
    fn for_resampler(resampler: &rubato::SincFixedOut<f64>) -> Self {
        Self {
            channel_buffers: resampler.output_buffer_allocate(true),
            frames_in_buffer: 0,
            next_channel: 0,
            next_frame: 0,
        }
    }

    fn empty_buffers(&mut self) -> &mut Vec<Vec<f64>> {
        &mut self.channel_buffers
    }

    fn trim_silent_end(&mut self) {
        let Some(longest_trimmed_len) = self
            .channel_buffers
            .iter()
            .take(self.frames_in_buffer)
            .map(|buf| {
                let silence = buf.iter().rev().take_while(|s| **s == 0f64).count();
                self.frames_in_buffer - silence
            })
            .max()
        else {
            return;
        };

        self.frames_in_buffer = longest_trimmed_len;
    }

    fn mark_filled(&mut self, frames_in_output: usize) {
        self.frames_in_buffer = frames_in_output;
        self.next_frame = 0;
    }
}

impl Iterator for ResamplerOutput {
    type Item = f64;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_frame >= self.frames_in_buffer {
            None
        } else {
            dbg!(self.frames_in_buffer);
            let sample = self
                .channel_buffers
                .get(self.next_channel)
                .expect("num channels larger then zero")
                .get(self.next_frame)?;
            self.next_channel = (self.next_frame + 1) % self.channel_buffers.len();
            self.next_frame += 1;
            Some(*sample)
        }
    }
}

pub struct SampleRateConverter<I>
where
    I: Iterator,
{
    input: I,

    // for size hint
    resample_ratio: f64,

    resampled: ResamplerOutput,
    resampler_input: Vec<Vec<f64>>,
    resampler: rubato::SincFixedOut<f64>,
}

impl<I> SampleRateConverter<I>
where
    I: Iterator,
    I::Item: Sample,
{
    #[inline]
    pub fn new(
        input: I,
        from: cpal::SampleRate,
        to: cpal::SampleRate,
        num_channels: cpal::ChannelCount,
    ) -> SampleRateConverter<I> {
        let from = from.0;
        let to = to.0;

        assert!(num_channels >= 1);
        assert!(from >= 1);
        assert!(to >= 1);

        let resample_ratio = to as f64 / from as f64;
        let max_resample_ratio_relative = 1.1;
        let window = rubato::WindowFunction::Blackman2;
        let sinc_len = 128;
        let params = SincInterpolationParameters {
            sinc_len,
            f_cutoff: rubato::calculate_cutoff(sinc_len, window),
            oversampling_factor: 256,
            interpolation: rubato::SincInterpolationType::Quadratic,
            window,
        };

        let resampler_chunk_size = 1024;
        let resampler = rubato::SincFixedOut::<f64>::new(
            resample_ratio,
            max_resample_ratio_relative,
            params,
            resampler_chunk_size,
            num_channels as usize,
        )
        .unwrap();

        SampleRateConverter {
            input,
            resample_ratio,
            resampled: ResamplerOutput::for_resampler(&resampler),
            resampler_input: resampler.input_buffer_allocate(false),
            resampler,
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

    fn fill_resampler_input(&mut self) {
        for channel_buffer in self.resampler_input.iter_mut() {
            channel_buffer.clear();
        }

        let needed_frames = self.resampler.input_frames_max();
        for _ in 0..needed_frames {
            for channel_buffer in self.resampler_input.iter_mut() {
                if let Some(item) = self.input.next() {
                    channel_buffer.push(item.to_f32() as f64);
                } else {
                    break;
                }
            }
        }
    }
}

impl<I> Iterator for SampleRateConverter<I>
where
    I: Iterator,
    I::Item: Sample + Clone,
{
    type Item = f64;

    fn next(&mut self) -> Option<f64> {
        if let Some(item) = self.resampled.next() {
            return Some(item);
        }

        self.fill_resampler_input();

        let input_len = self
            .resampler_input
            .get(0)
            .expect("num channels must be larger then zero")
            .len();

        if input_len == 0 {
            return None;
        }

        let mut padded_with_silence = false;
        if input_len < self.resampler.input_frames_max() {
            // resampler needs more frames then the input could provide,
            // pad with silence
            padded_with_silence = true;
            for channel in &mut self.resampler_input {
                channel.resize(self.resampler.input_frames_max(), 0f64);
            }
        }

        self.resampler_input
            .iter()
            .inspect(|buf| println!("{:?}", &buf[0..20]))
            .for_each(drop);

        let (_, frames_in_output) = self
            .resampler
            .process_into_buffer(
                &self.resampler_input,
                self.resampled.empty_buffers(),
                None, // all channels active
            )
            .expect("buffer sizes are correct");
        self.resampled.channel_buffers
            .iter()
            .inspect(|buf| println!("{:?}", &buf[0..20]))
            .for_each(drop);
        self.resampled.mark_filled(frames_in_output);


        if padded_with_silence {
            // remove padding
            self.resampled.trim_silent_end();
        }

        self.resampled.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower_bound, upper_bound) = self.input.size_hint();
        let lower_bound = (lower_bound as f64 * self.resample_ratio).floor() as usize;
        let upper_bound = upper_bound
            .map(|lower_bound| lower_bound as f64 * self.resample_ratio)
            .map(|lower_bound| lower_bound.ceil() as usize);
        (lower_bound, upper_bound)
    }
}
