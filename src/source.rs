use std::cmp;

use cpal;

use conversions::DataConverter;
use conversions::SamplesRateConverter;
use conversions::ChannelsCountConverter;

use Sample;

/// A source of samples.
// TODO: should be ExactSizeIterator
pub trait Source: Iterator where Self::Item: Sample {
    /// Returns the number of samples before the current channel ends.
    ///
    /// After the engine has finished reading the specified number of samples, it will assume that
    /// the value of `get_channels()` and/or `get_samples_rate()` have changed.
    fn get_current_frame_len(&self) -> usize;

    /// Returns the number of channels. Channels are always interleaved.
    fn get_channels(&self) -> u16;

    /// Returns the rate at which the source should be played.
    fn get_samples_rate(&self) -> u32;
}

/// An iterator that reads from a `Source` and converts the samples to a specific rate and
/// channels count.
///
/// It implements `Source` as well, but all the data is guaranteed to be in a single frame whose
/// channels and samples rate have been passed to `new`.
pub struct UniformSourceIterator<I, D> where I: Source, I::Item: Sample, D: Sample {
    inner: Option<DataConverter<ChannelsCountConverter<SamplesRateConverter<Take<I>>>, D>>,
    target_channels: u16,
    target_samples_rate: u32,
}

impl<I, D> UniformSourceIterator<I, D> where I: Source, I::Item: Sample, D: Sample {
    #[inline]
    pub fn new(input: I, target_channels: u16, target_samples_rate: u32)
               -> UniformSourceIterator<I, D>
    {
        let input = UniformSourceIterator::bootstrap(input, target_channels, target_samples_rate);

        UniformSourceIterator {
            inner: Some(input),
            target_channels: target_channels,
            target_samples_rate: target_samples_rate,
        }
    }

    #[inline]
    fn bootstrap(input: I, target_channels: u16, target_samples_rate: u32)
                 -> DataConverter<ChannelsCountConverter<SamplesRateConverter<Take<I>>>, D>
    {
        let frame_len = input.get_current_frame_len();

        let from_channels = input.get_channels();
        let from_samples_rate = input.get_samples_rate();

        let input = Take { iter: input, n: frame_len };
        let input = SamplesRateConverter::new(input, cpal::SamplesRate(from_samples_rate),
                                              cpal::SamplesRate(target_samples_rate),
                                              from_channels);
        let input = ChannelsCountConverter::new(input, from_channels, target_channels);
        let input = DataConverter::new(input);

        input
    }
}

impl<I, D> Iterator for UniformSourceIterator<I, D> where I: Source, I::Item: Sample, D: Sample {
    type Item = D;

    #[inline]
    fn next(&mut self) -> Option<D> {
        if let Some(value) = self.inner.as_mut().unwrap().next() {
            return Some(value);
        }

        let input = self.inner.take().unwrap().into_inner().into_inner().into_inner().iter;
        let mut input = UniformSourceIterator::bootstrap(input, self.target_channels,
                                                         self.target_samples_rate);

        let value = input.next();
        self.inner = Some(input);
        value
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.as_ref().unwrap().size_hint()
    }
}

impl<I, D> ExactSizeIterator for UniformSourceIterator<I, D> where I: ExactSizeIterator + Source, I::Item: Sample,
                                                                   D: Sample
{
}

impl<I, D> Source for UniformSourceIterator<I, D> where I: ExactSizeIterator + Source, I::Item: Sample, D: Sample {
    #[inline]
    fn get_current_frame_len(&self) -> usize {
        self.len()
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        self.target_channels
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        self.target_samples_rate
    }
}

/// The `Take` struct in the stdlib is missing `into_inner()`, so we reimplement it here.
struct Take<I> {
    iter: I,
    n: usize
}

impl<I> Iterator for Take<I> where I: Iterator {
    type Item = <I as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        if self.n != 0 {
            self.n -= 1;
            self.iter.next()
        } else {
            None
        }
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<I::Item> {
        if self.n > n {
            self.n -= n + 1;
            self.iter.nth(n)
        } else {
            if self.n > 0 {
                self.iter.nth(self.n - 1);
                self.n = 0;
            }
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, upper) = self.iter.size_hint();

        let lower = cmp::min(lower, self.n);

        let upper = match upper {
            Some(x) if x < self.n => Some(x),
            _ => Some(self.n)
        };

        (lower, upper)
    }
}

impl<I> ExactSizeIterator for Take<I> where I: ExactSizeIterator {
}
