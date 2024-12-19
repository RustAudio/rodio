use crate::{Sample, Source};
use std::time::Duration;

/// Simplest low pass filter with approximately sample_rate/2 cutoff.
/// See also tunable low pass filter implementations [crate::source::blt::low_pass]
/// and [crate::source::blt::low_pass_with_q].
pub struct SimpleLowPass<I>
where
    I: Iterator,
{
    input: I,
    prev: Option<I::Item>,
}

impl<I> SimpleLowPass<I>
where
    I: Iterator,
    I::Item: Sample,
{
    /// Create new simple low pass filter.
    #[inline]
    pub fn new(input: I) -> SimpleLowPass<I> {
        SimpleLowPass { input, prev: None }
    }
}

impl<I> Source for SimpleLowPass<I>
where
    I: Source,
    I::Item: Sample,
{
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.input.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
}

impl<I> Iterator for SimpleLowPass<I>
where
    I: Iterator,
    I::Item: Sample + Clone,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.input.next().map(|s| {
            let x = self.prev.unwrap_or(s).saturating_add(s).amplify(0.5);
            self.prev.replace(s);
            x
        })
    }
}
