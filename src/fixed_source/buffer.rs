use std::sync::Arc;
use std::time::Duration;

use crate::FixedSource;
use crate::{ChannelCount, Sample, SampleRate};

/// A buffer of samples treated as a source.
#[derive(Debug, Clone)]
pub struct SamplesBuffer {
    data: Arc<[Sample]>,
    pos: usize,
    channels: ChannelCount,
    sample_rate: SampleRate,
}

impl SamplesBuffer {
    /// Builds a new `SamplesBuffer`.
    pub fn new<D>(channels: ChannelCount, sample_rate: SampleRate, data: D) -> SamplesBuffer
    where
        D: Into<Vec<Sample>>,
    {
        let data: Arc<[f32]> = data.into().into();
        SamplesBuffer {
            data,
            pos: 0,
            channels,
            sample_rate,
        }
    }
}

impl FixedSource for SamplesBuffer {
    #[inline]
    fn channels(&self) -> ChannelCount {
        self.channels
    }
    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }
    /// # Panics
    /// If the length of the buffer is larger than approximately 16 billion elements.
    /// This is because the calculation of the duration would overflow.
    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        let duration_ns = 1_000_000_000u64
            .checked_mul(self.data.len() as u64)
            .unwrap()
            / self.sample_rate.get() as u64
            / self.channels.get() as u64;
        let duration = Duration::new(
            duration_ns / 1_000_000_000,
            (duration_ns % 1_000_000_000) as u32,
        );

        Some(duration)
    }
    // /// This jumps in memory till the sample for `pos`.
    // #[inline]
    // fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
    //     // This is fast because all the samples are in memory already
    //     // and due to the constant sample_rate we can jump to the right
    //     // sample directly.
    //     let curr_channel = self.pos % self.channels() as usize;
    //     let new_pos = pos.as_secs_f32() * self.sample_rate() as f32 * self.channels() as f32;
    //     // saturate pos at the end of the source
    //     let new_pos = new_pos as usize;
    //     let new_pos = new_pos.min(self.data.len());
    //     // make sure the next sample is for the right channel
    //     let new_pos = new_pos.next_multiple_of(self.channels() as usize);
    //     let new_pos = new_pos - curr_channel;
    //     self.pos = new_pos;
    //     Ok(())
    // }
}

impl Iterator for SamplesBuffer {
    type Item = Sample;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.data.get(self.pos)?;
        self.pos += 1;
        Some(*sample)
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.data.len(), Some(self.data.len()))
    }
}
