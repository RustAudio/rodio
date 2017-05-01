use std::time::Duration;

use Sample;
use Source;

/// Internal function that builds a `PeriodicAccess` object.
pub fn periodic<I, F>(source: I, period: Duration, modifier: F) -> PeriodicAccess<I, F>
    where I: Source,
          I::Item: Sample,
{
    // TODO: handle the fact that the samples rate can change
    // TODO: generally, just wrong
    let update_ms = period.as_secs() as u32 * 1_000 + period.subsec_nanos() / 1_000_000;
    let update_frequency = (update_ms * source.samples_rate()) / 1000;

    PeriodicAccess {
        input: source,
        modifier: modifier,
        update_frequency: update_frequency,
        samples_until_update: update_frequency,
    }
}

/// Calls a function on a source every time a period elapsed.
#[derive(Clone, Debug)]
pub struct PeriodicAccess<I, F> {
    // The inner source.
    input: I,

    // Closure that gets access to `inner`.
    modifier: F,

    // The frequency with which local_volume should be updated by remote_volume
    update_frequency: u32,

    // How many samples remain until it is time to update local_volume with remote_volume.
    samples_until_update: u32,
}

impl<I, F> Iterator for PeriodicAccess<I, F>
    where I: Source,
          I::Item: Sample,
          F: FnMut(&mut I)
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        self.samples_until_update -= 1;
        if self.samples_until_update == 0 {
            (self.modifier)(&mut self.input);
            self.samples_until_update = self.update_frequency;
        }

        self.input.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I, F> Source for PeriodicAccess<I, F>
    where I: Source,
          I::Item: Sample,
          F: FnMut(&mut I)
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        self.input.current_frame_len()
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.input.channels()
    }

    #[inline]
    fn samples_rate(&self) -> u32 {
        self.input.samples_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
}
