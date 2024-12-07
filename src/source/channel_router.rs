use std::{cmp::min, collections::HashMap};

use crate::{Sample, Source};

#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct InputOutputPair(u16, u16);

#[derive(Clone, Debug)]
pub struct ChannelRouter<I>
where
    I: Source,
    I::Item: Sample,
{
    input: I,
    channel_mappings: HashMap<InputOutputPair, f32>,
    current_channel: u16,
    channel_count: u16,
    input_buffer: Vec<I::Item>,
}

impl<I> ChannelRouter<I>
where
    I: Source,
    I::Item: Sample,
{
    pub fn new(
        input: I,
        channel_count: u16,
        channel_mappings: HashMap<InputOutputPair, f32>,
    ) -> Self {
        Self {
            input,
            channel_mappings,
            current_channel: 0u16,
            channel_count,
            input_buffer: vec![<I::Item as Sample>::zero_value(); channel_count.into()],
        }
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
    fn apply_gains(&self, output_channel: u16) -> I::Item {
        self.input_buffer
            .iter()
            .enumerate()
            .map(|(input_channel, in_sample)| {
                let pair = InputOutputPair(input_channel as u16, output_channel);
                let gain = self.channel_mappings.get(&pair).unwrap_or(&0.0f32);
                in_sample.amplify(*gain)
            })
            .reduce(|a, b| a.saturating_add(b))
            .unwrap_or(<I::Item as Sample>::zero_value())
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

        if self.input_buffer.len() == 0 {
            None
        } else {
            let retval = self.apply_gains(self.current_channel);
            self.current_channel += 1;
            Some(retval)
        }
    }
}
