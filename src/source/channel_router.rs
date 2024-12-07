use std::{cmp::min, collections::HashMap};

use crate::{Sample, Source};

pub type ChannelMap = HashMap<InputOutputPair, f32>;

pub fn channel_router<I>(input: I, channel_count: u16, channel_map: ChannelMap) -> ChannelRouter<I>
where
    I: Source,
    I::Item: Sample,
{
    ChannelRouter::new(input, channel_count, channel_map)
}

/// A tuple for describing the source and destination channel for a gain setting.
#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct InputOutputPair(u16, u16);

/// A source for extracting, reordering mixing and duplicating audio between
/// channels.
#[derive(Clone, Debug)]
pub struct ChannelRouter<I>
where
    I: Source,
    I::Item: Sample,
{
    input: I,
    channel_map: HashMap<InputOutputPair, f32>,
    current_channel: u16,
    channel_count: u16,
    input_buffer: Vec<I::Item>,
}

impl<I> ChannelRouter<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Creates a new [`ChannelRouter<I>`].
    ///
    /// The new `ChannelRouter` will read samples from `input` and will mix and map them according
    /// to `channel_mappings` into its output samples.
    pub fn new(input: I, channel_count: u16, channel_map: ChannelMap) -> Self {
        Self {
            input,
            channel_map,
            current_channel: channel_count, // this will cause the input buffer to fill on first
                                            // call to next()
            channel_count,
            input_buffer: vec![<I::Item as Sample>::zero_value(); 0],
        }
    }

    /// Add or update the gain setting for a channel mapping.
    pub fn map(&mut self, from: u16, to: u16, gain: f32) -> () {
        let k = InputOutputPair(from, to);
        self.channel_map.insert(k, gain);
    }
}

impl<I> Source for ChannelRouter<I>
where
    I: Source,
    I::Item: Sample,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.input.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.channel_count
    }

    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        self.input.total_duration()
    }
}

impl<I> ChannelRouter<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Create an output sample from the current input and the channel gain map
    fn render_output(&self) -> Option<I::Item> {
        self.input_buffer
            .iter()
            .enumerate()
            .map(|(input_channel, in_sample)| {
                let pair = InputOutputPair(input_channel as u16, self.current_channel);
                let gain = self.channel_map.get(&pair).unwrap_or(&0.0f32);
                in_sample.amplify(*gain)
            })
            .reduce(|a, b| a.saturating_add(b))
    }
}

impl<I> Iterator for ChannelRouter<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_channel >= self.channel_count {
            let input_channels = self.input.channels() as usize;
            let samples_to_take = min(
                input_channels,
                self.input.current_frame_len().unwrap_or(usize::MAX),
            );

            self.input_buffer = self.input.by_ref().take(samples_to_take).collect();

            self.current_channel = 0;
        }

        let retval = self.render_output();
        self.current_channel += 1;
        retval
    }
}
