use std::sync::Arc;
use std::time::Duration;

use crate::source::SeekError;
use crate::ConstSource;
use crate::Sample;

/// A buffer of samples treated as a source.
#[derive(Debug, Clone)]
pub struct SamplesBuffer<const SR: u32, const CH: u16> {
    data: Arc<[Sample]>,
    pos: usize,
}

impl<const SR: u32, const CH: u16> SamplesBuffer<SR, CH> {
    /// Builds a new `SamplesBuffer`.
    ///
    /// Note any call to total_duration will panic if the buffer is larger then
    /// 16 billion elements.
    pub fn new<D>(data: D) -> SamplesBuffer<SR, CH>
    where
        D: Into<Vec<Sample>>,
    {
        const { assert!(SR > 0) };
        const { assert!(CH > 0) };

        SamplesBuffer {
            data: data.into().into(),
            pos: 0,
        }
    }
}

impl<const SR: u32, const CH: u16> ConstSource<SR, CH> for SamplesBuffer<SR, CH> {
    /// # Panics
    /// If the length of the buffer is larger than approximately 16 billion elements.
    /// This is because the calculation of the duration would overflow.
    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        let duration_ns = 1_000_000_000u64
            .checked_mul(self.data.len() as u64)
            .unwrap()
            / SR as u64
            / CH as u64;
        Some(Duration::new(
            duration_ns / 1_000_000_000,
            (duration_ns % 1_000_000_000) as u32,
        ))
    }
    /// This jumps in memory till the sample for `pos`.
    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        // This is fast because all the samples are in memory already
        // and due to the constant sample_rate we can jump to the right
        // sample directly.
        let curr_channel = self.pos % self.channels().get() as usize;
        let new_pos =
            pos.as_secs_f32() * self.sample_rate().get() as f32 * self.channels().get() as f32;
        // saturate pos at the end of the source
        let new_pos = new_pos as usize;
        let new_pos = new_pos.min(self.data.len());
        // make sure the next sample is for the right channel
        let new_pos = new_pos.next_multiple_of(self.channels().get() as usize);
        let new_pos = new_pos - curr_channel;
        self.pos = new_pos;
        Ok(())
    }
}

impl<const SR: u32, const CH: u16> Iterator for SamplesBuffer<SR, CH> {
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
