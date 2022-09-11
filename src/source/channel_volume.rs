use std::time::Duration;

use crate::{Sample, Source};

/// Combines channels in input into a single mono source, then plays that mono sound
/// to each channel at the volume given for that channel.
#[derive(Clone, Debug)]
pub struct ChannelVolume<I>
where
    I: Source,
    I::Item: Sample,
{
    input: I,
    // Channel number is used as index for amplification value.
    channel_volumes: Vec<f32>,
    // Current listener being processed.
    current_channel: usize,
    current_sample: Option<I::Item>,
}

impl<I> ChannelVolume<I>
where
    I: Source,
    I::Item: Sample,
{
    pub fn new(mut input: I, channel_volumes: Vec<f32>) -> ChannelVolume<I>
    where
        I: Source,
        I::Item: Sample,
    {
        let mut sample = None;
        for _ in 0..input.channels() {
            if let Some(s) = input.next() {
                sample = Some(
                    sample
                        .get_or_insert_with(I::Item::zero_value)
                        .saturating_add(s),
                );
            }
        }
        ChannelVolume {
            input,
            channel_volumes,
            current_channel: 0,
            current_sample: sample,
        }
    }

    /// Sets the volume for a given channel number.  Will panic if channel number
    /// was invalid.
    pub fn set_volume(&mut self, channel: usize, volume: f32) {
        self.channel_volumes[channel] = volume;
    }

    /// Returns a reference to the inner source.
    #[inline]
    pub fn inner(&self) -> &I {
        &self.input
    }

    /// Returns a mutable reference to the inner source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.input
    }

    /// Returns the inner source.
    #[inline]
    pub fn into_inner(self) -> I {
        self.input
    }
}

impl<I> Iterator for ChannelVolume<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        // return value
        let ret = self
            .current_sample
            .map(|sample| sample.amplify(self.channel_volumes[self.current_channel]));
        self.current_channel += 1;
        if self.current_channel >= self.channel_volumes.len() {
            self.current_channel = 0;
            self.current_sample = None;
            for _ in 0..self.input.channels() {
                if let Some(s) = self.input.next() {
                    self.current_sample = Some(
                        self.current_sample
                            .get_or_insert_with(I::Item::zero_value)
                            .saturating_add(s),
                    );
                }
            }
        }
        ret
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let map_size_hint = |input_hint: usize| -> usize {
            // The input source provides a number of samples (input_hint / channels);
            // We return 1 item per channel per sample => * channel_volumes
            let input_provides =
                input_hint / (self.input.channels() as usize) * self.channel_volumes.len();

            // In addition, we may be in the process of emitting additional values from
            // self.current_sample
            let current_sample = if self.current_sample.is_some() {
                self.channel_volumes.len() - self.current_channel
            } else {
                0
            };

            input_provides + current_sample
        };

        let (min, max) = self.input.size_hint();
        (map_size_hint(min), max.map(map_size_hint))
    }
}

impl<I> ExactSizeIterator for ChannelVolume<I>
where
    I: Source + ExactSizeIterator,
    I::Item: Sample,
{
}

impl<I> Source for ChannelVolume<I>
where
    I: Source,
    I::Item: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        self.input.current_frame_len()
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.channel_volumes.len() as u16
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::SamplesBuffer;

    const SAMPLES: usize = 100;

    fn dummysource(channels: usize) -> SamplesBuffer<f32> {
        let data: Vec<f32> = (1..=(SAMPLES * channels)).map(|v| v as f32).collect();
        SamplesBuffer::new(channels as _, 1, data)
    }

    fn make_test(channels_source: usize, channels_result: usize) {
        let original = dummysource(channels_source);
        assert_eq!(original.size_hint().0, SAMPLES * channels_source);

        let mono = ChannelVolume::new(original, vec![1.0; channels_result]);

        let (hint_min, hint_max) = mono.size_hint();
        assert_eq!(Some(hint_min), hint_max);

        let actual_size = mono.count();
        assert_eq!(hint_min, actual_size);
    }

    #[test]
    fn size_stereo_mono() {
        make_test(2, 1);
    }
    #[test]
    fn size_mono_stereo() {
        make_test(1, 2);
    }

    #[test]
    fn size_stereo_eight() {
        make_test(2, 8);
    }
    #[test]
    fn size_stereo_five() {
        make_test(2, 5);
    }
}
