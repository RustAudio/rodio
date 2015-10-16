use std::cmp;
use std::time::Duration;
use std::mem;

use cpal;

use conversions::DataConverter;
use conversions::SamplesRateConverter;
use conversions::ChannelsCountConverter;

use Sample;

/// A source of samples.
pub trait Source: Iterator where Self::Item: Sample {
    /// Returns the number of samples before the current channel ends. `None` means "infinite".
    /// Should never return 0 unless there's no more data.
    ///
    /// After the engine has finished reading the specified number of samples, it will assume that
    /// the value of `get_channels()` and/or `get_samples_rate()` have changed.
    fn get_current_frame_len(&self) -> Option<usize>;

    /// Returns the number of channels. Channels are always interleaved.
    fn get_channels(&self) -> u16;

    /// Returns the rate at which the source should be played.
    fn get_samples_rate(&self) -> u32;

    /// Returns the total duration of this source, if known.
    ///
    /// `None` indicates at the same time "infinite" or "unknown".
    fn get_total_duration(&self) -> Option<Duration>;

    /// Repeats this source forever.
    ///
    /// Note that this works by storing the data in a buffer, so the amount of memory used is
    /// proportional to the size of the sound.
    #[inline]
    fn repeat_infinite(self) -> Repeat<Self> where Self: Sized {
        let buffer = vec![(Vec::new(), self.get_samples_rate(), self.get_channels())];
        Repeat { inner: RepeatImpl::FirstPass(self, buffer) }
    }
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
        (self.inner.as_ref().unwrap().size_hint().0, None)
    }
}

impl<I, D> Source for UniformSourceIterator<I, D> where I: Iterator + Source, I::Item: Sample,
                                                        D: Sample
{
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        self.target_channels
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        self.target_samples_rate
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        None
    }
}

struct Take<I> {
    iter: I,
    n: Option<usize>,
}

impl<I> Iterator for Take<I> where I: Iterator {
    type Item = <I as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        if let Some(ref mut n) = self.n {
            if *n != 0 {
                *n -= 1;
                self.iter.next()
            } else {
                None
            }

        } else {
            self.iter.next()
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if let Some(n) = self.n {
            let (lower, upper) = self.iter.size_hint();

            let lower = cmp::min(lower, n);

            let upper = match upper {
                Some(x) if x < n => Some(x),
                _ => Some(n)
            };

            (lower, upper)

        } else {
            self.iter.size_hint()
        }
    }
}

impl<I> ExactSizeIterator for Take<I> where I: ExactSizeIterator {
}

/// A source that repeats the given source.
pub struct Repeat<I> where I: Source, I::Item: Sample {
    inner: RepeatImpl<I>,
}

enum RepeatImpl<I> where I: Source, I::Item: Sample {
    FirstPass(I, Vec<(Vec<I::Item>, u32, u16)>),
    NextPasses(Vec<(Vec<I::Item>, u32, u16)>, usize, usize)
}

impl<I> Iterator for Repeat<I> where I: Source, I::Item: Sample {
    type Item = <I as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        match self.inner {
            RepeatImpl::FirstPass(ref mut input, ref mut buffer) => {
                match input.get_current_frame_len() {
                    Some(1) => {
                        if let Some(sample) = input.next() {
                            buffer.last_mut().unwrap().0.push(sample);
                            buffer.push((Vec::new(), input.get_samples_rate(), input.get_channels()));
                            return Some(sample);
                        }
                    },

                    Some(0) => {

                    },

                    _ => {
                        if let Some(sample) = input.next() {
                            buffer.last_mut().unwrap().0.push(sample);
                            return Some(sample);
                        }
                    },
                }
            },

            RepeatImpl::NextPasses(ref buffer, ref mut off1, ref mut off2) => {
                let sample = buffer[*off1].0[*off2];
                *off2 += 1;
                if *off2 >= buffer[*off1].0.len() {
                    *off1 += 1;
                    *off2 = 0;
                }
                if *off1 >= buffer.len() {
                    *off1 = 0;
                }
                return Some(sample);
            },
        }

        // if we reach this, we need to switch from FirstPass to NextPasses
        let buffer = if let RepeatImpl::FirstPass(_, ref mut buffer) = self.inner {
            mem::replace(buffer, Vec::new())
        } else {
            unreachable!()
        };

        mem::replace(&mut self.inner, RepeatImpl::NextPasses(buffer, 0, 0));
        self.next()
    }

    // TODO: size_hint
}

impl<I> Source for Repeat<I> where I: Iterator + Source, I::Item: Sample {
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        match self.inner {
            RepeatImpl::FirstPass(ref input, _) => input.get_current_frame_len(),
            RepeatImpl::NextPasses(ref buffers, off1, off2) => Some(buffers[off1].0.len() - off2),
        }
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        match self.inner {
            RepeatImpl::FirstPass(ref input, _) => input.get_channels(),
            RepeatImpl::NextPasses(ref buffers, off1, _) => buffers[off1].2,
        }
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        match self.inner {
            RepeatImpl::FirstPass(ref input, _) => input.get_samples_rate(),
            RepeatImpl::NextPasses(ref buffers, off1, _) => buffers[off1].1,
        }
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        // TODO: ?
        None
    }
}
