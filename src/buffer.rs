//! A simple source of samples coming from a buffer.
//!
//! The `SamplesBuffer` struct can be used to treat a list of values as a `Source`.
//!
//! # Example
//!
//! ```
//! use rodio::buffer::SamplesBuffer;
//! let _ = SamplesBuffer::new(1, 44100, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
//! ```
//!

use crate::common::{ChannelCount, SampleRate};
use crate::source::SeekError;
use crate::{Sample, Source};
use std::time::Duration;

/// A buffer of samples treated as a source.
#[derive(Debug, Clone)]
pub struct SamplesBuffer {
    data: Vec<Sample>,
    pos: usize,
    channels: ChannelCount,
    sample_rate: SampleRate,
    duration: Duration,
}

impl SamplesBuffer {
    /// Builds a new `SamplesBuffer`.
    ///
    /// # Panic
    ///
    /// - Panics if the number of channels is zero.
    /// - Panics if the samples rate is zero.
    /// - Panics if the length of the buffer is larger than approximately 16 billion elements.
    ///   This is because the calculation of the duration would overflow.
    ///
    pub fn new<D>(channels: ChannelCount, sample_rate: SampleRate, data: D) -> SamplesBuffer
    where
        D: Into<Vec<Sample>>,
    {
        assert!(channels >= 1);
        assert!(sample_rate >= 1);

        let data = data.into();
        let duration_ns = 1_000_000_000u64.checked_mul(data.len() as u64).unwrap()
            / sample_rate as u64
            / channels as u64;
        let duration = Duration::new(
            duration_ns / 1_000_000_000,
            (duration_ns % 1_000_000_000) as u32,
        );

        SamplesBuffer {
            data,
            pos: 0,
            channels,
            sample_rate,
            duration,
        }
    }
}

impl Source for SamplesBuffer {
    #[inline]
    fn parameters_changed(&self) -> bool {
        None
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.channels
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        Some(self.duration)
    }

    /// This jumps in memory till the sample for `pos`.
    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        // This is fast because all the samples are in memory already
        // and due to the constant sample_rate we can jump to the right
        // sample directly.

        let curr_channel = self.pos % self.channels() as usize;
        let new_pos = pos.as_secs_f32() * self.sample_rate() as f32 * self.channels() as f32;
        // saturate pos at the end of the source
        let new_pos = new_pos as usize;
        let new_pos = new_pos.min(self.data.len());

        // make sure the next sample is for the right channel
        let new_pos = new_pos.next_multiple_of(self.channels() as usize);
        let new_pos = new_pos - curr_channel;

        self.pos = new_pos;
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use crate::buffer::SamplesBuffer;
    use crate::source::Source;

    #[test]
    fn basic() {
        let _ = SamplesBuffer::new(1, 44100, vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    #[should_panic]
    fn panic_if_zero_channels() {
        SamplesBuffer::new(0, 44100, vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    #[should_panic]
    fn panic_if_zero_sample_rate() {
        SamplesBuffer::new(1, 0, vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn duration_basic() {
        let buf = SamplesBuffer::new(2, 2, vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let dur = buf.total_duration().unwrap();
        assert_eq!(dur.as_secs(), 1);
        assert_eq!(dur.subsec_nanos(), 500_000_000);
    }

    #[test]
    fn iteration() {
        let mut buf = SamplesBuffer::new(1, 44100, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(buf.next(), Some(1.0));
        assert_eq!(buf.next(), Some(2.0));
        assert_eq!(buf.next(), Some(3.0));
        assert_eq!(buf.next(), Some(4.0));
        assert_eq!(buf.next(), Some(5.0));
        assert_eq!(buf.next(), Some(6.0));
        assert_eq!(buf.next(), None);
    }

    #[cfg(test)]
    mod try_seek {
        use super::*;
        use crate::common::{ChannelCount, SampleRate};
        use crate::Sample;
        use std::time::Duration;

        #[test]
        fn channel_order_stays_correct() {
            const SAMPLE_RATE: SampleRate = 100;
            const CHANNELS: ChannelCount = 2;
            let mut buf = SamplesBuffer::new(
                CHANNELS,
                SAMPLE_RATE,
                (0..2000i16)
                    .into_iter()
                    .map(|s| s as Sample)
                    .collect::<Vec<_>>(),
            );
            buf.try_seek(Duration::from_secs(5)).unwrap();
            assert_eq!(buf.next(), Some(5.0 * SAMPLE_RATE as f32 * CHANNELS as f32));

            assert!(buf.next().is_some_and(|s| s.trunc() as i32 % 2 == 1));
            assert!(buf.next().is_some_and(|s| s.trunc() as i32 % 2 == 0));

            buf.try_seek(Duration::from_secs(6)).unwrap();
            assert!(buf.next().is_some_and(|s| s.trunc() as i32 % 2 == 1),);
        }
    }
}
