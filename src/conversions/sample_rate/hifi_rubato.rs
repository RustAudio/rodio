// high fidelity resampler using the rubato crate. In contradiction to the
// fast in house resampler this does not provide ExactSizeIterator. We cannot
// do that since rubato does not guaranteed the amount of samples returned

use std::vec;

use rubato::{Resampler, SincInterpolationParameters};

use crate::Sample;

/// Rubato requires the samples for each channel to be in separate buffers.
/// This wrapper around Vec<Vec<f64>> provides an iterator that returns
/// samples interleaved.
struct Resampled {
    channel_buffers: Vec<Vec<f64>>,
    next_channel: usize,
    next_frame: usize,
}

impl Resampled {
    fn new(channels: u16, capacity_per_channel: usize) -> Self {
        Self {
            channel_buffers: vec![Vec::with_capacity(capacity_per_channel); channels as usize],
            next_channel: 0,
            next_frame: 0,
        }
    }

    fn empty_buffers(&mut self) -> &mut Vec<Vec<f64>> {
        self.channel_buffers.clear();
        &mut self.channel_buffers
    }
}

impl Iterator for Resampled {
    type Item = f64;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.channel_buffers[self.next_channel].get(self.next_frame)?;
        self.next_channel = (self.next_frame + 1) % self.channel_buffers.len();
        self.next_frame += 1;

        Some(*sample)
    }
}

pub struct SampleRateConverter<I>
where
    I: Iterator,
{
    input: I,

    // for size hint
    resample_ratio: f64,

    resampled: Resampled,
    /// in number of audio frames where one frame is all the samples
    /// for all channels.
    resampler_chunk_size: usize,
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

        let resampler_input_size = 1024;
        let resampler_output_size =
            ((resampler_input_size as f64) * resample_ratio).ceil() as usize;
        let resampler = rubato::SincFixedOut::<f64>::new(
            resample_ratio,
            max_resample_ratio_relative,
            params,
            resampler_input_size,
            num_channels as usize,
        )
        .unwrap();

        SampleRateConverter {
            input,
            resample_ratio,
            resampled: Resampled::new(num_channels, resampler_output_size),
            resampler_chunk_size: 1024,
            resampler,
            resampler_input: vec![Vec::with_capacity(resampler_input_size); num_channels as usize],
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
        self.resampler_input.clear();
        for _ in 0..self.resampler_chunk_size {
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

        if self.resampler_input.len() >= self.resampler_chunk_size {
            self.resampler
                .process_into_buffer(
                    &self.resampler_input,
                    self.resampled.empty_buffers(),
                    None, // all channels active
                )
                .expect(
                    "Input and output have correct number of channels, \
                    input is long enough",
                );
        } else {
            self.resampler
                    // gets the last samples out of the resampler
                .process_partial_into_buffer(
                    // might have to pass in None if the input is empty
                    // something to check if this fails near the end of a source
                    Some(&self.resampler_input),
                    self.resampled.empty_buffers(),
                    None, // all channels active
                )
                .expect("Input and output have correct number of channels, \
                    input is long enough");
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
