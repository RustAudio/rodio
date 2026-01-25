use std::time::Duration;

use dasp_sample::Sample as _;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::{Float, Sample, Source};

/// Combines channels in input into a single mono source, then plays that mono sound
/// to each channel at the volume given for that channel.
#[derive(Clone, Debug)]
pub struct ChannelVolume<I>
where
    I: Source,
{
    input: I,
    channel_volumes: Vec<Float>,
    current_channel: usize,
    current_sample: Option<Sample>,
}

impl<I> ChannelVolume<I>
where
    I: Source,
{
    /// Wrap the input source and make it mono. Play that mono sound to each
    /// channel at the volume set by the user. The volume can be changed using
    /// [`ChannelVolume::set_volume`].
    pub fn new(input: I, channel_volumes: Vec<Float>) -> ChannelVolume<I> {
        let channel_count = channel_volumes.len(); // See next() implementation.
        ChannelVolume {
            input,
            channel_volumes,
            current_channel: channel_count,
            current_sample: None,
        }
    }

    /// Sets the volume for a given channel number. Will panic if channel number
    /// is invalid.
    pub fn set_volume(&mut self, channel: usize, volume: Float) {
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
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_channel >= self.channel_volumes.len() {
            self.current_channel = 0;
            self.current_sample = None;
            for _ in 0..self.input.channels().get() {
                let s = self.input.next()?;
                self.current_sample = Some(self.current_sample.unwrap_or(Sample::EQUILIBRIUM) + s);
            }
            self.current_sample = self
                .current_sample
                .map(|s| s / self.input.channels().get() as Float);
        }
        let result = self
            .current_sample
            .map(|s| s * self.channel_volumes[self.current_channel]);
        self.current_channel += 1;
        result
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for ChannelVolume<I> where I: Source + ExactSizeIterator {}

impl<I> Source for ChannelVolume<I>
where
    I: Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(self.channel_volumes.len() as u16)
            .expect("checked to be non-empty in new implementation")
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.input.try_seek(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::nz;
    use crate::source::test_utils::TestSource;

    #[test]
    fn test_mono_to_stereo() {
        let input = TestSource::new(&[1.0, 2.0, 3.0], nz!(1), nz!(44100));
        let mut channel_vol = ChannelVolume::new(input, vec![0.5, 0.8]);
        assert_eq!(channel_vol.next(), Some(1.0 * 0.5));
        assert_eq!(channel_vol.next(), Some(1.0 * 0.8));
        assert_eq!(channel_vol.next(), Some(2.0 * 0.5));
        assert_eq!(channel_vol.next(), Some(2.0 * 0.8));
        assert_eq!(channel_vol.next(), Some(3.0 * 0.5));
        assert_eq!(channel_vol.next(), Some(3.0 * 0.8));
        assert_eq!(channel_vol.next(), None);
    }

    #[test]
    fn test_stereo_to_mono() {
        let input = TestSource::new(&[1.0, 2.0, 3.0, 4.0], nz!(2), nz!(44100));
        let mut channel_vol = ChannelVolume::new(input, vec![1.0]);
        assert_eq!(channel_vol.next(), Some(1.5));
        assert_eq!(channel_vol.next(), Some(3.5));
        assert_eq!(channel_vol.next(), None);
    }

    #[test]
    fn test_stereo_to_stereo_with_mixing() {
        let input = TestSource::new(&[1.0, 3.0, 2.0, 4.0], nz!(2), nz!(44100));
        let mut channel_vol = ChannelVolume::new(input, vec![0.5, 2.0]);
        assert_eq!(channel_vol.next(), Some(2.0 * 0.5)); // 1.0
        assert_eq!(channel_vol.next(), Some(2.0 * 2.0)); // 4.0
        assert_eq!(channel_vol.next(), Some(3.0 * 0.5)); // 1.5
        assert_eq!(channel_vol.next(), Some(3.0 * 2.0)); // 6.0
        assert_eq!(channel_vol.next(), None);
    }
}
