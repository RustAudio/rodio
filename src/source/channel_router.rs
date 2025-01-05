// Channel router types and implementation.

use crate::{ChannelCount, Sample, Source};
use dasp_sample::{Sample as DaspSample, ToSample};
use std::{
    error::Error,
    fmt,
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
pub type ChannelMap = Vec<ChannelLink>;
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
    channel_map: &ChannelMap,
) -> (ChannelRouterController, ChannelRouterSource<I>)
where
    I: Source,
    I::Item: Sample,
{
    ChannelRouterSource::new(input, channel_count, channel_map)
}

/// `ChannelRouterController::map()` returns this error if the router source has been dropped.
#[derive(Debug, Eq, PartialEq)]
pub struct ChannelRouterControllerError {}

impl fmt::Display for ChannelRouterControllerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<ChannelRouterControllerError>")
    }
}

impl Error for ChannelRouterControllerError {}

/// A controller type that sends gain updates to a corresponding [`ChannelRouterSource`].
#[derive(Debug, Clone)]
pub struct ChannelRouterController {
    sender: Sender<ChannelMap>,
}

impl ChannelRouterController {
    /// Set or update the gain setting for a channel mapping.
    pub fn set_map(&mut self, channel_map: &ChannelMap) -> Result<(), impl Error> {
        self.sender.send(channel_map.clone())
    }
}

/// (source_channel, target_channel, gain)
pub type ChannelLink = (ChannelCount, ChannelCount, f32);
// Alternatively it can be a struct but map construction becomes more verbose:
// #[derive(Debug, Copy, Clone)]
// pub struct ChannelLink {
//     pub from: ChannelCount,
//     pub to: ChannelCount,
//     pub gain: f32,
// }

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
    output_frame: Vec<Option<I::Item>>,

    /// Communication channel with the controller
    receiver: Receiver<ChannelMap>,
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
        channel_map: &ChannelMap,
    ) -> (ChannelRouterController, Self) {
        let mut channel_map = channel_map.to_owned();
        Self::prepare_map(&mut channel_map);

        let (tx, rx) = channel();
        let controller = ChannelRouterController { sender: tx };
        let source = Self {
            input,
            channel_map,
            // this will cause the input buffer to fill on first call to next()
            current_channel: channel_count,
            channel_count,
            /// I::Item::zero_value() zero value is not 0 for some sample types
            output_frame: vec![None; channel_count.into()],
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

    fn prepare_map(channel_map: &mut ChannelMap) {
        channel_map.sort_by(|a, b| a.0.cmp(&b.0))
    }
}

impl<I> Source for ChannelRouterSource<I>
where
    I: Source,
    I::Item: Sample,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
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
            self.current_channel = 0;
            self.output_frame.fill(None);
            let input_channels = self.input.channels() as usize;
            let mut li = 0;
            let input_frame: Vec<I::Item> = self.inner_mut().take(input_channels).collect();
            if input_frame.len() < input_channels {
                return None;
            }
            for (ch_in, s) in input_frame.iter().enumerate() {
                while li < self.channel_map.len() {
                    let link = &self.channel_map[li];
                    if link.0 > ch_in as u16 {
                        break;
                    } else if link.0 == ch_in as u16 {
                        let amplified = s.amplify(link.2);
                        let mut c = &mut self.output_frame[link.1 as usize];
                        *c = Some(c.map_or(amplified, |x| x.saturating_add(amplified)));
                    }
                    li += 1;
                }
            }

            if let Some(mut map_update) = self.receiver.try_iter().last() {
                Self::prepare_map(&mut map_update);
                self.channel_map = map_update;
            }
        }
        let sample = self.output_frame[self.current_channel as usize];
        self.current_channel += 1;
        Some(sample.unwrap_or(I::Item::zero_value()).to_sample())
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
        let map = vec![(0, 0, 0.5f32), (1, 0, 0.5f32)];

        let (_, test_source) = ChannelRouterSource::new(input, 1, &map);
        let v1: Vec<u16> = test_source.take(4).collect();
        assert_eq!(v1, [1u16, 5u16]);
    }

    #[test]
    fn test_upmix() {
        let input = SamplesBuffer::new(1, 1, [0i16, -10, 10, 20, -20, -50, -30, 40]);
        let map = vec![(0, 0, 1.0f32), (0, 1, 0.5f32), (0, 2, 2.0f32)];
        let (_, test_source) = ChannelRouterSource::new(input, 3, &map);
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
        let mut map = vec![(0, 0, 1.0f32), (1, 0, 1.0f32)];
        let (mut controller, mut source) = ChannelRouterSource::new(input, 1, &map);
        let v1: Vec<i16> = source.by_ref().take(2).collect();
        assert_eq!(v1, vec![0i16, -2i16]);

        map[0].2 = 0.0f32;
        map[1].2 = 2.0f32;
        assert!(controller.set_map(&map).is_ok());

        let v2: Vec<i16> = source.take(3).collect();
        assert_eq!(v2, vec![4i16, -6i16]);
    }
}
