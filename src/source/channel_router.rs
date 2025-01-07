// Channel router types and implementation.

use crate::{ChannelCount, Sample, Source};
use dasp_sample::Sample as DaspSample;
use std::cell::Cell;
use std::{
    error::Error,
    fmt,
    sync::mpsc::{channel, Receiver, Sender},
};

/// Weighted connection between an input and an output channel.
/// (source_channel, target_channel, gain)
// Alternatively this can be a struct but map construction becomes more verbose.
pub type ChannelLink = (ChannelCount, ChannelCount, f32);

/// An input channels to output channels mapping.
pub type ChannelMap = Vec<ChannelLink>;

/// Function that builds a [`ChannelMixer`] object.
/// The new `ChannelMixer` will read samples from `input` and will mix and map them according
/// to `channel_mappings` into its output samples.
pub fn channel_mixer<I>(
    input: I,
    out_channels_count: u16,
    channel_map: &ChannelMap,
) -> (ChannelMixer, ChannelMixerSource<I>)
where
    I: Source,
    I::Item: Sample,
{
    let (tx, rx) = channel();
    let controller = ChannelMixer {
        sender: tx,
        out_channels_count,
    };
    let source = ChannelMixerSource {
        input,
        channel_map: vec![],
        // this will cause the input buffer to fill on first call to next()
        current_channel: out_channels_count,
        channel_count: out_channels_count,
        input_frame: Cell::new(vec![]),
        // I::Item::zero_value() zero value is not 0 for some sample types,
        // so have to use an option.
        output_frame: vec![None; out_channels_count.into()],
        receiver: rx,
    };
    // TODO Return an error here? Requires to change API. Alternatively,
    //      map can be set separately, or this can panic.
    controller
        .set_map(channel_map)
        .expect("set channel mixer map");

    (controller, source)
}

/// `ChannelRouterController::map()` returns this error if the router source has been dropped.
#[derive(Debug, Eq, PartialEq)]
pub enum ChannelMixerError {
    ConfigError,
    SendError,
}

impl fmt::Display for ChannelMixerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<ChannelMixerError>")
    }
}

impl Error for ChannelMixerError {}

/// A controller type that sends gain updates to a corresponding [`ChannelMixerSource`].
#[derive(Debug, Clone)]
pub struct ChannelMixer {
    sender: Sender<ChannelMap>,
    out_channels_count: ChannelCount,
}

impl ChannelMixer {
    /// Set or update the gain setting for a channel mapping.
    pub fn set_map(&self, channel_map: &ChannelMap) -> Result<(), ChannelMixerError> {
        let mut new_map = channel_map.clone();
        self.prepare_map(&mut new_map)?;
        self.sender
            .send(new_map)
            .map_err(|_| ChannelMixerError::SendError)
    }

    fn prepare_map(&self, new_channel_map: &mut ChannelMap) -> Result<(), ChannelMixerError> {
        if !new_channel_map
            .iter()
            .all(|(_from, to, _gain)| to < &self.out_channels_count)
        {
            return Err(ChannelMixerError::ConfigError);
        }
        new_channel_map.retain(|(_from, _to, gain)| *gain != 0.0);
        new_channel_map.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(())
    }
}

/// A source for extracting, reordering, mixing and duplicating audio between
/// channels.
// #[derive(Debug)] // TODO Reimplement debug? A Cell is not Debug.
pub struct ChannelMixerSource<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Input [`Source`]
    input: I,

    /// Mapping of input to output channels.
    channel_map: ChannelMap,

    /// The output channel that [`next()`] will return next.
    current_channel: u16,

    /// The number of output channels.
    channel_count: u16,

    /// Helps to reduce dynamic allocation.
    input_frame: Cell<Vec<I::Item>>,

    /// The current input audio frame.
    output_frame: Vec<Option<I::Item>>,

    /// Communication channel with the controller.
    receiver: Receiver<ChannelMap>,
}

impl<I> ChannelMixerSource<I>
where
    I: Source,
    I::Item: Sample,
{
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

impl<I> Source for ChannelMixerSource<I>
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

impl<I> Iterator for ChannelMixerSource<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_channel >= self.channel_count {
            // TODO One may want to change mapping when incoming channel count changes.
            if let Some(map_update) = self.receiver.try_iter().last() {
                self.channel_map = map_update;
            }

            self.current_channel = 0;
            self.output_frame.fill(None);
            let input_channels = self.input.channels() as usize;

            let mut input_frame = self.input_frame.take();
            input_frame.truncate(0);
            input_frame.extend(self.inner_mut().take(input_channels));
            if input_frame.len() < input_channels {
                return None;
            }
            let mut li = 0;
            for (ch_in, s) in input_frame.iter().enumerate() {
                while li < self.channel_map.len() {
                    let link = &self.channel_map[li];
                    if link.0 > ch_in as u16 {
                        break;
                    }
                    if link.0 == ch_in as u16 {
                        let amplified = s.amplify(link.2);
                        let c = &mut self.output_frame[link.1 as usize];
                        // This can be simpler if samples had a way to get additive zero (0, or 0.0).
                        *c = Some(c.map_or(amplified, |x| x.saturating_add(amplified)));
                    }
                    li += 1;
                }
            }
            self.input_frame.replace(input_frame);
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

        let (_, test_source) = channel_mixer(input, 1, &map);
        let v1: Vec<u16> = test_source.take(4).collect();
        assert_eq!(v1, [1u16, 5u16]);
    }

    #[test]
    fn test_upmix() {
        let input = SamplesBuffer::new(1, 1, [0i16, -10, 10, 20, -20, -50, -30, 40]);
        let map = vec![(0, 0, 1.0f32), (0, 1, 0.5f32), (0, 2, 2.0f32)];
        let (_, test_source) = channel_mixer(input, 3, &map);
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
        let (mut controller, mut source) = channel_mixer(input, 1, &map);
        let v1: Vec<i16> = source.by_ref().take(2).collect();
        assert_eq!(v1, vec![0i16, -2i16]);

        map[0].2 = 0.0f32;
        map[1].2 = 2.0f32;
        assert!(controller.set_map(&map).is_ok());

        let v2: Vec<i16> = source.take(3).collect();
        assert_eq!(v2, vec![4i16, -6i16]);
    }

    #[test]
    fn test_arbitrary_mixing() {
        let input = SamplesBuffer::new(4, 1, [10i16, 100, 300, 700, 1100, 1300, 1705].repeat(4));
        // 4 to 3 channels.
        let map = vec![
            // Intentionally left 1 without mapping to test the case.
            (2, 0, 1.0f32),
            (3, 0, 0.1f32),
            (3, 1, 0.3f32),
            (0, 1, 0.7f32),
            (0, 2, 0.6f32),
            // For better diagnostics this should be rejected, currently it is ignored.
            (17, 0, 321.5f32),
        ];
        let (_controller, mut source) = channel_mixer(input, 3, &map);
        let v1: Vec<i16> = source.by_ref().collect();
        assert_eq!(v1.len(), 21);
        assert_eq!(
            v1,
            vec![
                370i16, 217, 6, 1706, 773, 660, 810, 400, 60, 20, 940, 780, 1230, 600, 180, 130,
                1283, 1023, 1470, 1001, 420
            ]
        );
    }
}
