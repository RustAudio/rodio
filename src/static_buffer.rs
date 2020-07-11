//! A simple source of samples coming from a static buffer.
//!
//! The `StaticSamplesBuffer` struct can be used to treat a list of values as a `Source`.
//!
//! # Example
//!
//! ```
//! use rodio::static_buffer::StaticSamplesBuffer;
//! let _ = StaticSamplesBuffer::new(1, 44100, &[1i16, 2, 3, 4, 5, 6]);
//! ```
//!

use std::slice::Iter as SliceIter;
use std::time::Duration;

use crate::source::Source;
use crate::Sample;

/// A buffer of samples treated as a source.
#[derive(Clone)]
pub struct StaticSamplesBuffer<S>
where
    S: 'static,
{
    data: SliceIter<'static, S>,
    channels: u16,
    sample_rate: u32,
    duration: Duration,
}

impl<S> StaticSamplesBuffer<S>
where
    S: Sample,
{
    /// Builds a new `StaticSamplesBuffer`.
    ///
    /// # Panic
    ///
    /// - Panicks if the number of channels is zero.
    /// - Panicks if the samples rate is zero.
    /// - Panicks if the length of the buffer is larger than approximatively 16 billion elements.
    ///   This is because the calculation of the duration would overflow.
    ///
    pub fn new(channels: u16, sample_rate: u32, data: &'static [S]) -> StaticSamplesBuffer<S> {
        assert!(channels != 0);
        assert!(sample_rate != 0);

        let duration_ns = 1_000_000_000u64.checked_mul(data.len() as u64).unwrap()
            / sample_rate as u64
            / channels as u64;
        let duration = Duration::new(
            duration_ns / 1_000_000_000,
            (duration_ns % 1_000_000_000) as u32,
        );

        StaticSamplesBuffer {
            data: data.iter(),
            channels: channels,
            sample_rate: sample_rate,
            duration: duration,
        }
    }
}

impl<S> Source for StaticSamplesBuffer<S>
where
    S: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.channels
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        Some(self.duration)
    }
}

impl<S> Iterator for StaticSamplesBuffer<S>
where
    S: Sample + Clone,
{
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        self.data.next().cloned()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.data.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use crate::source::Source;
    use crate::static_buffer::StaticSamplesBuffer;

    #[test]
    fn basic() {
        let _ = StaticSamplesBuffer::new(1, 44100, &[0i16, 0, 0, 0, 0, 0]);
    }

    #[test]
    #[should_panic]
    fn panic_if_zero_channels() {
        StaticSamplesBuffer::new(0, 44100, &[0i16, 0, 0, 0, 0, 0]);
    }

    #[test]
    #[should_panic]
    fn panic_if_zero_sample_rate() {
        StaticSamplesBuffer::new(1, 0, &[0i16, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn duration_basic() {
        let buf = StaticSamplesBuffer::new(2, 2, &[0i16, 0, 0, 0, 0, 0]);
        let dur = buf.total_duration().unwrap();
        assert_eq!(dur.as_secs(), 1);
        assert_eq!(dur.subsec_nanos(), 500_000_000);
    }

    #[test]
    fn iteration() {
        let mut buf = StaticSamplesBuffer::new(1, 44100, &[1i16, 2, 3, 4, 5, 6]);
        assert_eq!(buf.next(), Some(1));
        assert_eq!(buf.next(), Some(2));
        assert_eq!(buf.next(), Some(3));
        assert_eq!(buf.next(), Some(4));
        assert_eq!(buf.next(), Some(5));
        assert_eq!(buf.next(), Some(6));
        assert_eq!(buf.next(), None);
    }
}
