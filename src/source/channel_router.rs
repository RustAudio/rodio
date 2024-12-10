// Channel router types and implementation.

use crate::{Sample, Source};
use std::{
    cmp::min,
    sync::mpsc::{channel, Receiver, Sender},
};

/// A matrix to map inputs to outputs according to a gain
///
/// A two-dimensional matrix of `f32`s:
/// - The first dimension is respective to the input channels
/// - The second is respective to the output channels
///
/// Thus, if a value at `map[1,1]` is 0.2, this signifies that the signal on
/// channel 1 should be mixed into channel 1 with a coefficient of 0.2.
pub type ChannelMap = Vec<Vec<f32>>;
// doing this as Vec<Vec<atomic_float::AtomicF32>> would require feature=experimental, so I decided
// to just use a channel to do updates.
//
// Doing it as a HashMap<(u16,u16), f32> is an option too but there's a penalty hashing these
// values, there's ways to speed that up though. It'd be great if the object upgraded its
// implementation if it got sufficiently big.

// pub fn empty_channel_map(inputs: u16, outputs: u16) -> ChannelMap {
//     vec![vec![0.0f32; outputs.into()]; inputs.into()]
// }

/// Internal function that builds a [`ChannelRouter<I>`] object.
pub fn channel_router<I>(
    input: I,
    channel_count: u16,
    channel_map: ChannelMap,
) -> (ChannelRouterController, ChannelRouterSource<I>)
where
    I: Source,
    I::Item: Sample,
{
    ChannelRouterSource::new(input, channel_count, channel_map)
}

struct ChannelRouterMessage(usize, usize, f32);

/// `ChannelRouterController::map()` returns this error if the router source has been dropped.
pub struct ChannelRouterControllerError {}

/// A controller type that sends gain updates to a corresponding [`ChannelRouterSource`].
#[derive(Debug, Clone)]
pub struct ChannelRouterController {
    sender: Sender<ChannelRouterMessage>,
}

impl ChannelRouterController {
    /// Set or update the gain setting for a channel mapping.
    ///
    /// A channel from the input may be routed to any number of channels in the output, and a
    /// channel in the output may be a mix of any number of channels in the input.
    ///
    /// Successive calls to `mix` with the same `from` and `to` arguments will replace the
    /// previous gain value with the new one.
    pub fn map(
        &mut self,
        from: u16,
        to: u16,
        gain: f32,
    ) -> Result<(), ChannelRouterControllerError> {
        if self
            .sender
            .send(ChannelRouterMessage(from as usize, to as usize, gain))
            .is_err()
        {
            Err(ChannelRouterControllerError {})
        } else {
            Ok(())
        }
    }
}

/// A source for extracting, reordering, mixing and duplicating audio between
/// channels.
#[derive(Debug)]
pub struct ChannelRouterSource<I>
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

    /// Communication channel with the controller
    receiver: Receiver<ChannelRouterMessage>,
}

impl<I> ChannelRouterSource<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Creates a new [`ChannelRouter<I>`].
    ///
    /// The new `ChannelRouter` will read samples from `input` and will mix and map them according
    /// to `channel_mappings` into its output samples.
    ///
    /// # Panics
    ///
    /// - if `channel_count` is not equal to `channel_map`'s second dimension
    /// - if `input.channels()` is not equal to `channel_map`'s first dimension
    pub fn new(
        input: I,
        channel_count: u16,
        channel_map: ChannelMap,
    ) -> (ChannelRouterController, Self) {
        assert!(channel_count as usize == channel_map[0].len());
        assert!(input.channels() as usize == channel_map.len());

        let (tx, rx) = channel();

        let controller = ChannelRouterController { sender: tx };
        let source = Self {
            input,
            channel_map,
            current_channel: channel_count,
            // this will cause the input buffer to fill on first call to next()
            channel_count,
            // channel_count is redundant, it's implicit in the channel_map dimensions
            // but maybe it's saving us some time, we do check this value a lot.
            input_buffer: vec![],
            receiver: rx,
        };

        (controller, source)
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

impl<I> Source for ChannelRouterSource<I>
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
        self.channel_count
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.input.total_duration()
    }
}

impl<I> Iterator for ChannelRouterSource<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
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

            for change in self.receiver.try_iter() {
                self.channel_map[change.0][change.1] = change.2;
            }
        }

        // Find the output sample for current_channel
        let retval = self
            .input_buffer
            .iter()
            .zip(&self.channel_map)
            .map(|(in_sample, input_gains)| {
                // the way this works, the input_buffer need not be totally full, the router will
                // work with whatever samples are available and the missing samples will be assumed
                // to be equilibrium.
                let gain = input_gains[self.current_channel as usize];
                in_sample.amplify(gain)
            })
            .reduce(|a, b| a.saturating_add(b));

        self.current_channel += 1;
        retval
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use crate::buffer::SamplesBuffer;
    use crate::source::channel_router::*;

    #[test]
    fn test_stereo_to_mono() {
        let input = SamplesBuffer::new(2, 1, [0u16, 2u16, 4u16, 6u16]);
        let map = vec![vec![0.5f32], vec![0.5f32]];

        let (_, test_source) = ChannelRouterSource::new(input, 1, map);
        let v1: Vec<u16> = test_source.take(4).collect();
        assert_eq!(v1.len(), 2);
        assert_eq!(v1[0], 1u16);
        assert_eq!(v1[1], 5u16);
    }

    #[test]
    fn test_upmix() {
        let input = SamplesBuffer::new(1, 1, [0i16, -10, 10, 20, -20, -50, -30, 40]);
        let map = vec![vec![1.0f32, 0.5f32, 2.0f32]];
        let (_, test_source) = ChannelRouterSource::new(input, 3, map);
        assert_eq!(test_source.channels(), 3);
        let v1: Vec<i16> = test_source.take(1000).collect();
        assert_eq!(v1.len(), 24);
        assert_eq!(
            v1,
            [
                0i16, 0, 0, -10, -5, -20, 10, 5, 20, 20, 10, 40, -20, -10, -40, -50, -25, -100,
                -30, -15, -60, 40, 20, 80
            ]
        );
    }

    #[test]
    fn test_updates() {
        let input = SamplesBuffer::new(2, 1, [0i16, 0i16, -1i16, -1i16, 1i16, 2i16, -4i16, -3i16]);
        let initial_map = vec![vec![1.0f32], vec![1.0f32]];
        let (mut controller, mut source) = ChannelRouterSource::new(input, 1, initial_map);
        let v1: Vec<i16> = source.by_ref().take(2).collect();
        assert_eq!(v1.len(), 2);
        assert_eq!(v1[0], 0i16);
        assert_eq!(v1[1], -2i16);

        controller.map(0, 0, 0.0f32);
        controller.map(1, 0, 2.0f32);

        let v2: Vec<i16> = source.take(3).collect();
        assert_eq!(v2.len(), 2);

        assert_eq!(v2[0], 4i16);
        assert_eq!(v2[1], -6i16);
    }
}
