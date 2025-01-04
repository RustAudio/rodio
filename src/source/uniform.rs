use std::cmp;
use std::marker::PhantomData;
use std::time::Duration;

use cpal::FromSample;

// #[cfg(not(feature = "experimental-hifi-resampler"))]
// #[cfg(feature = "experimental-hifi-resampler")]
use crate::conversions::sample_rate::Resampler;
use crate::conversions::ChannelCountConverter;
use crate::{Sample, Source};

use super::SeekError;

/// An iterator that reads from a `Source` and converts the samples to a
/// specific type, sample-rate and channels count.
///
/// It implements `Source` as well, but all the data is guaranteed to be in a
/// single frame whose channels and samples rate have been passed to `new`.
pub struct UniformSourceIterator<I, D, R>
where
    I: Source,
    I::Item: Sample,
    D: FromSample<I::Item> + Sample,
    R: Resampler<TakeFrame<I>, D>,
{
    // recreated each frame as each frame the channel
    // count and sample rate may change.
    /// only none while setting up the next frame
    inner: Option<ChannelCountConverter<R>>,
    target_channels: u16,
    target_sample_rate: u32,
    total_duration: Option<Duration>,
    source_type: PhantomData<I>,
    output_type: PhantomData<D>,
}

impl<I, D, R> UniformSourceIterator<I, D, R>
where
    I: Source,
    I::Item: Sample,
    D: FromSample<I::Item> + Sample,
    R: Resampler<TakeFrame<I>, D>,
{
    /// Wrap a `Source` and lazily convert its samples to a specific type,
    /// sample-rate and channels count.
    #[inline]
    pub fn new(
        input: I,
        target_channels: u16,
        target_sample_rate: u32,
    ) -> UniformSourceIterator<I, D, R> {
        let total_duration = input.total_duration();
        let resampler_parts = R::new_parts();
        let input =
            Self::convert_frame(input, resampler_parts, target_channels, target_sample_rate);

        UniformSourceIterator {
            inner: Some(input),
            target_channels,
            target_sample_rate,
            total_duration,
            source_type: PhantomData,
            output_type: PhantomData,
        }
    }

    #[inline]
    fn convert_frame(
        input: I,
        resampler_parts: R::Parts,
        target_channels: u16,
        target_sample_rate: u32,
    ) -> ChannelCountConverter<R> {
        // Limit the frame length to something reasonable
        let frame_len = input.current_frame_len().map(|x| x.min(32768));

        let from_channels = input.channels();
        let from_sample_rate = input.sample_rate();

        let input = TakeFrame {
            inner: input,
            n: frame_len,
        };
        let input = R::from_parts(
            input,
            resampler_parts,
            cpal::SampleRate(from_sample_rate),
            cpal::SampleRate(target_sample_rate),
            from_channels,
        );
        ChannelCountConverter::new(input, from_channels, target_channels)
    }
}

impl<I, D, R> Iterator for UniformSourceIterator<I, D, R>
where
    I: Source,
    I::Item: Sample,
    D: FromSample<I::Item> + Sample,
    R: Resampler<TakeFrame<I>, D>,
{
    type Item = D;

    #[inline]
    fn next(&mut self) -> Option<D> {
        if let Some(value) = self.inner.as_mut().expect("not setting up frame").next() {
            return Some(value);
        }

        let channel_count_converter = self.inner.take().expect("not setting up frame");
        let resampler = channel_count_converter.into_inner();
        let (single_frame, resampler_parts) = resampler.into_source_and_parts();
        let source = single_frame.into_inner();

        let mut input = UniformSourceIterator::convert_frame(
            source,
            resampler_parts,
            self.target_channels,
            self.target_sample_rate,
        );

        let value = input.next();
        self.inner = Some(input);
        value
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.inner.as_ref().unwrap().size_hint().0, None)
    }
}

impl<I, D, R> Source for UniformSourceIterator<I, D, R>
where
    I: Iterator + Source,
    I::Item: Sample,
    D: FromSample<I::Item> + Sample,
    R: Resampler<TakeFrame<I>, D>,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.target_channels
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.target_sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        if let Some(input) = self.inner.as_mut() {
            input // UniformSourceIterator
                .inner_mut() // ChannelCountConverter
                .inner_mut() // Resampler
                .inner_mut() // TakeFrame
                .try_seek(pos)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug)]
pub struct TakeFrame<S>
where
    S: Source,
    <S as Iterator>::Item: crate::Sample,
{
    inner: S,
    n: Option<usize>,
}

impl<S> TakeFrame<S>
where
    S: Source,
    <S as Iterator>::Item: crate::Sample,
{
    #[inline]
    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    #[inline]
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: Source> Iterator for TakeFrame<S>
where
    S: Source,
    <S as Iterator>::Item: crate::Sample,
{
    type Item = <S as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<<S as Iterator>::Item> {
        if let Some(n) = &mut self.n {
            if *n != 0 {
                *n -= 1;
                self.inner.next()
            } else {
                None
            }
        } else {
            self.inner.next()
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if let Some(n) = self.n {
            let (lower, upper) = self.inner.size_hint();

            let lower = cmp::min(lower, n);

            let upper = match upper {
                Some(x) if x < n => Some(x),
                _ => Some(n),
            };

            (lower, upper)
        } else {
            self.inner.size_hint()
        }
    }
}
