use std::time::Duration;

use Source;
use Sample;

/// Internal function that builds a `Repeat` object.
pub fn take_duration<I>(input: I, duration: Duration) -> TakeDuration<I>
    where I: Source,
          I::Item: Sample
{
    TakeDuration {
        input: input,
        remaining_duration: duration,
        requested_duration: duration,
    }
}

/// A source that repeats the given source.
#[derive(Clone, Debug)]
pub struct TakeDuration<I>
    where I: Source,
          I::Item: Sample
{
    input: I,
    remaining_duration: Duration,
    requested_duration: Duration,
}

impl<I> TakeDuration<I>
    where I: Source,
          I::Item: Sample
{
    /// Returns the duration elapsed for each sample extracted.
    #[inline]
    fn get_duration_per_sample(&self) -> Duration {
        let ns = 1000000000 / (self.input.get_samples_rate() as u64 * self.get_channels() as u64);
        // \|/ the maximum value of `ns` is one billion, so this can't fail
        Duration::new(0, ns as u32)
    }
}

impl<I> Iterator for TakeDuration<I>
    where I: Source,
          I::Item: Sample
{
    type Item = <I as Iterator>::Item;

    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        let duration_per_sample = self.get_duration_per_sample();

        if self.remaining_duration <= duration_per_sample {
            None

        } else {
            if let Some(sample) = self.input.next() {
                self.remaining_duration = self.remaining_duration - duration_per_sample;
                Some(sample)

            } else {
                None
            }
        }
    }

    // TODO: size_hint
}

impl<I> Source for TakeDuration<I>
    where I: Iterator + Source,
          I::Item: Sample
{
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        if let Some(value) = self.input.get_current_frame_len() {
            let remaining_nanosecs = self.remaining_duration.as_secs() * 1000000000 +
                                     self.remaining_duration.subsec_nanos() as u64;
            let remaining_samples = remaining_nanosecs * self.input.get_samples_rate() as u64 *
                                    self.get_channels() as u64 /
                                    1000000000;

            if (value as u64) < remaining_samples {
                Some(value)
            } else {
                Some(remaining_samples as usize)
            }

        } else {
            None
        }
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        self.input.get_channels()
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        self.input.get_samples_rate()
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        if let Some(duration) = self.input.get_total_duration() {
            if duration < self.requested_duration {
                Some(duration)
            } else {
                Some(self.requested_duration)
            }

        } else {
            None
        }
    }
}
