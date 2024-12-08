use std::{cmp::min, collections::HashMap};

use crate::{Sample, Source};

/// A tuple for describing the source and destination channel for a gain setting.
#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct InputOutputPair(u16, u16);

/// A [`HashMap`] for defining a connection between an input channel and output channel, and the
/// gain to apply to that connection.
pub type ChannelMap = HashMap<InputOutputPair, f32>;

/// Internal function that builds a [`ChannelRouter<I>`] object.
pub fn channel_router<I>(input: I, channel_count: u16, channel_map: ChannelMap) -> ChannelRouter<I>
where
    I: Source,
    I::Item: Sample,
{
    ChannelRouter::new(input, channel_count, channel_map)
}

/// A source for extracting, reordering, mixing and duplicating audio between
/// channels.
#[derive(Clone, Debug)]
pub struct ChannelRouter<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Input [`Source`]
    input: I,

    /// Mapping of input to output channels
    channel_map: ChannelMap,

    /// The output channel that [`next()`] will return next.
    current_channel: u16,

    /// The number of output channels
    channel_count: u16,

    /// The current input audio frame
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
            current_channel: channel_count,
            // this will cause the input buffer to fill on first call to next()
            channel_count,
            input_buffer: vec![],
        }
    }

    /// Set or update the gain setting for a channel mapping.
    ///
    /// A channel from the input may be routed to any number of channels in the output, and a
    /// channel in the output may be a mix of any number of channels in the input.
    ///
    /// Successive calls to `route` with the same `from` and `to` arguments will replace the
    /// previous gain value with the new one.
    pub fn route(&mut self, from: u16, to: u16, gain: f32) -> () {
        let k = InputOutputPair(from, to);
        _ = self.channel_map.insert(k, gain);
    }

    /// Delete an existing mapping from `from` to `to` if it exists.
    pub fn unroute(&mut self, from: u16, to: u16) -> () {
        let k = InputOutputPair(from, to);
        _ = self.channel_map.remove(&k);
    }

    /// Destroys this router and returns the underlying source.
    #[inline]
    pub fn into_inner(self) -> I {
        self.input
    }

    /// Get mutable access to the inner source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.input
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

impl<I> Iterator for ChannelRouter<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_channel >= self.channel_count {
            // We've reached the end of the frame, time to grab another one from the input
            let input_channels = self.input.channels() as usize;

            // This might be too fussy, a source should never break a frame in the middle of an
            // audio frame.
            let samples_to_take = min(
                input_channels,
                self.input.current_frame_len().unwrap_or(usize::MAX),
            );

            // fill the input buffer. If the input is exhausted and returning None this will make
            // the input buffer zero length
            self.input_buffer = self.inner_mut().take(samples_to_take).collect();

            self.current_channel = 0;
        }

        // Find the output sample for current_channel
        let retval = self
            .input_buffer
            .iter()
            // if input_buffer is empty, retval will be None
            .enumerate()
            .map(|(input_channel, in_sample)| {
                // the way this works, the input_buffer need not be totally full, the router will
                // work with whatever samples are available and the missing samples will be assumed
                // to be equilibrium.
                let pair = InputOutputPair(input_channel as u16, self.current_channel);
                let gain = self.channel_map.get(&pair).unwrap_or(&0.0f32);
                in_sample.amplify(*gain)
            })
            .reduce(|a, b| a.saturating_add(b));

        self.current_channel += 1;
        retval
    }
}
