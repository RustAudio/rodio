use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::math::NANOS_PER_SEC;
use crate::Source;

fn whole_frames(duration: Duration, sample_rate: SampleRate) -> u128 {
    duration.as_nanos() * u128::from(sample_rate.get()) / u128::from(NANOS_PER_SEC)
}

/// Internal function that builds a `Delay` object.
pub fn delay<I>(input: I, duration: Duration) -> Delay<I>
where
    I: Source,
{
    let silence_channels = input.channels();
    let silence_rate = input.sample_rate();
    let channels = usize::from(silence_channels.get());
    let silence_frames =
        whole_frames(duration, silence_rate).min((usize::MAX / channels) as u128) as usize;

    Delay {
        input,
        remaining_samples: silence_frames * channels,
        silence_frames,
        silence_channels,
        silence_rate,
    }
}

/// A source that delays the given source by a certain amount.
#[derive(Clone, Debug)]
pub struct Delay<I> {
    input: I,
    remaining_samples: usize,
    silence_frames: usize,
    silence_channels: ChannelCount,
    silence_rate: SampleRate,
}

impl<I> Delay<I>
where
    I: Source,
{
    fn silence_duration(&self) -> Duration {
        let nanos_per_sec = u128::from(NANOS_PER_SEC);
        // Use the first representable timestamp at or after the end of the silence.
        let nanos = (self.silence_frames as u128 * nanos_per_sec)
            .div_ceil(u128::from(self.silence_rate.get()));
        Duration::new(
            (nanos / nanos_per_sec) as u64,
            (nanos % nanos_per_sec) as u32,
        )
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

impl<I> Iterator for Delay<I>
where
    I: Source,
{
    type Item = <I as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        if self.remaining_samples >= 1 {
            self.remaining_samples -= 1;
            Some(0.0)
        } else {
            self.input.next()
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (min, max) = self.input.size_hint();
        (
            min + self.remaining_samples,
            max.map(|v| v + self.remaining_samples),
        )
    }
}

impl<I> ExactSizeIterator for Delay<I> where I: Iterator + Source + ExactSizeIterator {}

impl<I> Source for Delay<I>
where
    I: Iterator + Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        if self.remaining_samples > 0 {
            Some(self.silence_frames * usize::from(self.silence_channels.get()))
        } else {
            self.input.current_span_len()
        }
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        if self.remaining_samples > 0 {
            self.silence_channels
        } else {
            self.input.channels()
        }
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        if self.remaining_samples > 0 {
            self.silence_rate
        } else {
            self.input.sample_rate()
        }
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input
            .total_duration()
            .and_then(|duration| duration.checked_add(self.silence_duration()))
    }

    /// Pos is seen from the perspective of the api user.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::time::Duration;
    ///
    /// let mut source = inner_source.delay(Duration::from_secs(10));
    /// source.try_seek(Duration::from_secs(15));
    ///
    /// // inner_source is now at pos: Duration::from_secs(5);
    /// ```
    ///
    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.input
            .try_seek(pos.saturating_sub(self.silence_duration()))?;
        let elapsed_frames = whole_frames(pos, self.silence_rate);
        let remaining_frames =
            (self.silence_frames as u128).saturating_sub(elapsed_frames) as usize;
        self.remaining_samples = remaining_frames * usize::from(self.silence_channels.get());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::SamplesBuffer;
    use crate::math::nz;

    #[test]
    fn delay_emits_whole_silent_frames() {
        let source =
            SamplesBuffer::new(nz!(2), nz!(2), [0.25, -0.25]).delay(Duration::from_millis(250));
        assert_eq!(source.current_span_len(), Some(2));
        assert_eq!(source.collect::<Vec<_>>(), [0.25, -0.25]);
    }

    #[test]
    fn delay_uses_the_quantized_duration_for_seeking() {
        let mut source = SamplesBuffer::new(nz!(2), nz!(2), [0.25, -0.25, 0.5, -0.5])
            .delay(Duration::from_millis(750));
        assert_eq!(source.total_duration(), Some(Duration::from_millis(1500)));

        source.try_seek(Duration::from_millis(250)).unwrap();
        assert_eq!(source.next(), Some(0.0));
        assert_eq!(source.current_span_len(), Some(2));
        assert_eq!(source.next(), Some(0.0));

        source.try_seek(Duration::from_secs(1)).unwrap();
        assert_eq!(source.collect::<Vec<_>>(), [0.5, -0.5]);
    }
}
