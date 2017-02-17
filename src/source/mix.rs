use std::cmp;
use std::time::Duration;

use source::uniform::UniformSourceIterator;

use Sample;
use Source;

/// Internal function that builds a `Mix` object.
pub fn mix<I1, I2>(input1: I1, input2: I2) -> Mix<I1, I2>
    where I1: Source,
          I1::Item: Sample,
          I2: Source,
          I2::Item: Sample
{
    let channels = input1.get_channels();
    let rate = input1.get_samples_rate();

    Mix {
        input1: UniformSourceIterator::new(input1, channels, rate),
        input2: UniformSourceIterator::new(input2, channels, rate),
    }
}

/// Filter that modifies each sample by a given value.
#[derive(Clone)]
pub struct Mix<I1, I2>
    where I1: Source,
          I1::Item: Sample,
          I2: Source,
          I2::Item: Sample
{
    input1: UniformSourceIterator<I1, I1::Item>,
    input2: UniformSourceIterator<I2, I1::Item>,
}

impl<I1, I2> Iterator for Mix<I1, I2>
    where I1: Source,
          I1::Item: Sample,
          I2: Source,
          I2::Item: Sample
{
    type Item = I1::Item;

    #[inline]
    fn next(&mut self) -> Option<I1::Item> {
        let s1 = self.input1.next();
        let s2 = self.input2.next();

        // FIXME: shouldn't lerp, this is wrong
        match (s1, s2) {
            (Some(s1), Some(s2)) => Some(Sample::lerp(s1, s2, 1, 2)),
            (Some(s1), None) => Some(Sample::lerp(s1, Sample::zero_value(), 1, 2)),
            (None, Some(s2)) => Some(Sample::lerp(s2, Sample::zero_value(), 1, 2)),
            (None, None) => None,
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let s1 = self.input1.size_hint();
        let s2 = self.input2.size_hint();

        let min = cmp::max(s1.0, s2.0);
        let max = match (s1.1, s2.1) {
            (Some(s1), Some(s2)) => Some(cmp::max(s1, s2)),
            _ => None,
        };

        (min, max)
    }
}

impl<I1, I2> ExactSizeIterator for Mix<I1, I2>
    where I1: Source + ExactSizeIterator,
          I1::Item: Sample,
          I2: Source + ExactSizeIterator,
          I2::Item: Sample
{
}

impl<I1, I2> Source for Mix<I1, I2>
    where I1: Source,
          I1::Item: Sample,
          I2: Source,
          I2::Item: Sample
{
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        let f1 = self.input1.get_current_frame_len();
        let f2 = self.input2.get_current_frame_len();

        match (f1, f2) {
            (Some(f1), Some(f2)) => Some(cmp::min(f1, f2)),
            _ => None,
        }
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        self.input1.get_channels()
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        self.input1.get_samples_rate()
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        let f1 = self.input1.get_total_duration();
        let f2 = self.input2.get_total_duration();

        match (f1, f2) {
            (Some(f1), Some(f2)) => Some(cmp::max(f1, f2)),
            _ => None,
        }
    }
}
