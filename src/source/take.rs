use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::math::NANOS_PER_SEC;
use crate::{Float, Sample, Source};

/// Internal function that builds a `TakeDuration` object.
pub fn take_duration<I>(input: I, duration: Duration) -> TakeDuration<I>
where
    I: Source,
{
    let sample_rate = input.sample_rate();
    let channels = input.channels();
    TakeDuration {
        duration_per_sample: TakeDuration::get_duration_per_sample(&input),
        input,
        remaining_duration: duration,
        requested_duration: duration,
        filter: None,
        last_sample_rate: sample_rate,
        last_channels: channels,
        samples_counted: 0,
        current_span_len: None,
    }
}

/// A filter that can be applied to a `TakeDuration`.
#[derive(Clone, Debug)]
enum DurationFilter {
    FadeOut,
}
impl DurationFilter {
    fn apply<I: Iterator>(&self, sample: Sample, parent: &TakeDuration<I>) -> Sample {
        match self {
            DurationFilter::FadeOut => {
                let remaining = parent.remaining_duration.as_millis() as Float;
                let total = parent.requested_duration.as_millis() as Float;
                sample * remaining / total
            }
        }
    }
}

/// A source that truncates the given source to a certain duration.
#[derive(Clone, Debug)]
pub struct TakeDuration<I> {
    input: I,
    remaining_duration: Duration,
    requested_duration: Duration,
    filter: Option<DurationFilter>,
    // Cached duration per sample, updated when sample rate or channels change.
    duration_per_sample: Duration,
    last_sample_rate: SampleRate,
    last_channels: ChannelCount,
    samples_counted: usize,
    current_span_len: Option<usize>,
}

impl<I> TakeDuration<I>
where
    I: Source,
{
    /// Returns the duration elapsed for each sample extracted.
    #[inline]
    fn get_duration_per_sample(input: &I) -> Duration {
        let ns = NANOS_PER_SEC / (input.sample_rate().get() as u64 * input.channels().get() as u64);
        // \|/ the maximum value of `ns` is one billion, so this can't fail
        Duration::new(0, ns as u32)
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

    /// Make the truncated source end with a FadeOut. The fadeout covers the
    /// entire length of the take source.
    pub fn set_filter_fadeout(&mut self) {
        self.filter = Some(DurationFilter::FadeOut);
    }

    /// Remove any filter set.
    pub fn clear_filter(&mut self) {
        self.filter = None;
    }
}

impl<I> Iterator for TakeDuration<I>
where
    I: Source,
{
    type Item = <I as Iterator>::Item;

    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        if self.remaining_duration < self.duration_per_sample {
            None
        } else if let Some(sample) = self.input.next() {
            self.samples_counted = self.samples_counted.saturating_add(1);

            let input_span_len = self.input.current_span_len();
            let current_sample_rate = self.input.sample_rate();
            let current_channels = self.input.channels();

            // If input reports no span length, then by contract parameters are stable.
            let mut parameters_changed = false;
            let at_boundary = input_span_len.is_some_and(|_| {
                let known_boundary = self
                    .current_span_len
                    .map(|cached_len| self.samples_counted >= cached_len);

                // In span-counting mode, the only way parameters can change is at a span boundary.
                // In detection mode after try_seek, we check every sample until we detect a boundary.
                if known_boundary.is_none_or(|at_boundary| at_boundary) {
                    parameters_changed = current_channels != self.last_channels
                        || current_sample_rate != self.last_sample_rate;
                }

                known_boundary.unwrap_or(parameters_changed)
            });

            if at_boundary {
                if parameters_changed {
                    self.last_sample_rate = current_sample_rate;
                    self.last_channels = current_channels;
                    self.duration_per_sample = Self::get_duration_per_sample(&self.input);
                }

                self.samples_counted = 0;
                self.current_span_len = input_span_len;
            }

            let sample = match &self.filter {
                Some(filter) => filter.apply(sample, self),
                None => sample,
            };

            self.remaining_duration -= self.duration_per_sample;

            Some(sample)
        } else {
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining_nanos = self.remaining_duration.as_secs() * 1_000_000_000
            + self.remaining_duration.subsec_nanos() as u64;
        let nanos_per_sample = self.duration_per_sample.as_secs() * 1_000_000_000
            + self.duration_per_sample.subsec_nanos() as u64;

        if nanos_per_sample == 0 || remaining_nanos == 0 {
            return (0, Some(0));
        }

        let remaining_samples = (remaining_nanos / nanos_per_sample) as usize;

        let (inner_lower, inner_upper) = self.input.size_hint();
        let lower = inner_lower.min(remaining_samples);
        let upper = inner_upper
            .map(|u| u.min(remaining_samples))
            .or(Some(remaining_samples));

        (lower, upper)
    }
}

impl<I> ExactSizeIterator for TakeDuration<I> where I: Source + ExactSizeIterator {}

impl<I> Source for TakeDuration<I>
where
    I: Iterator + Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        let remaining_nanos = self.remaining_duration.as_secs() * NANOS_PER_SEC
            + self.remaining_duration.subsec_nanos() as u64;
        let nanos_per_sample = self.duration_per_sample.as_secs() * NANOS_PER_SEC
            + self.duration_per_sample.subsec_nanos() as u64;

        if nanos_per_sample == 0 || remaining_nanos == 0 {
            return Some(0);
        }

        let remaining_samples = (remaining_nanos / nanos_per_sample) as usize;

        self.input
            .current_span_len()
            .filter(|value| *value < remaining_samples)
            .or(Some(remaining_samples))
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.input.channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        if let Some(duration) = self.input.total_duration() {
            if duration < self.requested_duration {
                Some(duration)
            } else {
                Some(self.requested_duration)
            }
        } else {
            None
        }
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        let result = self.input.try_seek(pos);
        if result.is_ok() {
            // Recalculate remaining duration after seek
            self.remaining_duration = self.requested_duration.saturating_sub(pos);
            self.samples_counted = 0;

            // After seeking, we may be mid-span at an unknown position.
            // Special case: seeking to Duration::ZERO means we're at the start of the first span.
            // Otherwise, enter detection mode (current_span_len = None) to check parameters
            // every sample until we detect a span boundary by parameter change.
            if pos == Duration::ZERO {
                self.current_span_len = self.input.current_span_len();
            } else {
                self.current_span_len = None;
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SineWave;

    #[test]
    fn test_size_hint_with_zero_remaining() {
        let source = SineWave::new(440.0).take_duration(Duration::ZERO);
        assert_eq!(source.size_hint(), (0, Some(0)));
    }

    #[test]
    fn test_exact_duration_boundary() {
        use crate::source::SineWave;

        let sample_rate = 48000;
        let nanos_per_sample = (1_000_000_000 as Float / sample_rate as Float) as usize;

        let n_samples = 10;
        let exact_duration = Duration::from_nanos((nanos_per_sample * n_samples) as u64);

        let source = SineWave::new(440.0).take_duration(exact_duration);

        let count = source.count();
        assert_eq!(count, n_samples);
    }
}
