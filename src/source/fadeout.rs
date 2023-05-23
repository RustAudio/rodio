use std::time::Duration;

use Sample;
use Source;

/// Internal function that builds a `FadeOut` object.
pub fn fadeout<I>(input: I, duration: Duration) -> FadeOut<I> {
    let duration = duration.as_secs() * 1000000000 + duration.subsec_nanos() as u64;

    FadeOut {
        input: input,
        remaining_ns: duration as f32,
        total_ns: duration as f32,
    }
}

/// Filter that modifies reduces the volume to silence over a time period.
#[derive(Clone, Debug)]
pub struct FadeOut<I> {
    input: I,
    remaining_ns: f32,
    total_ns: f32,
}

impl<I> FadeOut<I> {
    /// Starts the fade to silence.
    #[inline]
    pub fn start(&mut self) {
        self.remaining_ns = self.total_ns;
    }

    /// Clears the fade out time.
    #[inline]
    pub fn reset(&mut self) {
        self.remaining_ns = -1.0;
    }
}

impl<I> Iterator for FadeOut<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        let factor = if self.remaining_ns <= 0.0 {
            0.
        } else {
            let factor = self.remaining_ns / self.total_ns;
            self.remaining_ns -=
                1000000000.0 / (self.input.sample_rate() as f32 * self.channels() as f32);
            factor
        };

        self.input.next().map(|value| value.amplify(factor))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for FadeOut<I>
where
    I: Source + ExactSizeIterator,
    I::Item: Sample,
{
}

impl<I> Source for FadeOut<I>
where
    I: Source,
    I::Item: Sample,
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
    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
}
