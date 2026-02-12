use std::time::Duration;

use super::resample::{Poly, Resample, ResampleConfig};
use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::conversions::ChannelCountConverter;
use crate::Source;

#[derive(Clone)]
enum UniformInner<I: Source> {
    Passthrough(I),
    SampleRate(Resample<I>),
    ChannelCount(ChannelCountConverter<I>),
    BothUpmix(ChannelCountConverter<Resample<I>>),
    BothDownmix(Resample<ChannelCountConverter<I>>),
}

impl<I: Source> Iterator for UniformInner<I> {
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            UniformInner::Passthrough(take) => take.next(),
            UniformInner::SampleRate(converter) => converter.next(),
            UniformInner::ChannelCount(converter) => converter.next(),
            UniformInner::BothUpmix(converter) => converter.next(),
            UniformInner::BothDownmix(converter) => converter.next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            UniformInner::Passthrough(take) => take.size_hint(),
            UniformInner::SampleRate(converter) => converter.size_hint(),
            UniformInner::ChannelCount(converter) => converter.size_hint(),
            UniformInner::BothUpmix(converter) => converter.size_hint(),
            UniformInner::BothDownmix(converter) => converter.size_hint(),
        }
    }
}

impl<I: Source> UniformInner<I> {
    #[inline]
    fn into_inner(self) -> I {
        match self {
            UniformInner::Passthrough(source) => source,
            UniformInner::SampleRate(converter) => converter.into_inner(),
            UniformInner::ChannelCount(converter) => converter.into_inner(),
            UniformInner::BothUpmix(converter) => converter.into_inner().into_inner(),
            UniformInner::BothDownmix(converter) => converter.into_inner().into_inner(),
        }
    }

    #[inline]
    fn inner(&self) -> &I {
        match self {
            UniformInner::Passthrough(source) => source,
            UniformInner::SampleRate(converter) => converter.inner(),
            UniformInner::ChannelCount(converter) => converter.inner(),
            UniformInner::BothUpmix(converter) => converter.inner().inner(),
            UniformInner::BothDownmix(converter) => converter.inner().inner(),
        }
    }

    #[inline]
    fn inner_mut(&mut self) -> &mut I {
        match self {
            UniformInner::Passthrough(source) => source,
            UniformInner::SampleRate(converter) => converter.inner_mut(),
            UniformInner::ChannelCount(converter) => converter.inner_mut(),
            UniformInner::BothUpmix(converter) => converter.inner_mut().inner_mut(),
            UniformInner::BothDownmix(converter) => converter.inner_mut().inner_mut(),
        }
    }
}

impl<I: Source> Source for UniformInner<I> {
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.inner().current_span_len()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.inner().channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.inner().sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.inner().total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner_mut().try_seek(pos)
    }
}

/// An iterator that reads from a `Source` and converts the samples to a
/// specific type, sample-rate and channels count.
///
/// It implements `Source` as well, but all the data is guaranteed to be in a
/// single span whose channels and samples rate have been passed to `new`.
#[derive(Clone)]
pub struct UniformSourceIterator<I>
where
    I: Source,
{
    inner: Option<UniformInner<I>>,
    target_channels: ChannelCount,
    target_sample_rate: SampleRate,
    current_channels: ChannelCount,
    current_sample_rate: SampleRate,
    current_span_pos: usize,
    cached_span_len: Option<usize>,
}

impl<I> UniformSourceIterator<I>
where
    I: Source,
{
    /// Wrap a `Source` and lazily convert its samples to a specific type,
    /// sample-rate and channels count.
    pub fn new(
        input: I,
        target_channels: ChannelCount,
        target_sample_rate: SampleRate,
    ) -> UniformSourceIterator<I> {
        let current_channels = input.channels();
        let current_sample_rate = input.sample_rate();
        let inner = UniformSourceIterator::bootstrap(input, target_channels, target_sample_rate);
        let cached_span_len = inner.current_span_len();

        Self {
            inner: Some(inner),
            target_channels,
            target_sample_rate,
            current_channels,
            current_sample_rate,
            current_span_pos: 0,
            cached_span_len,
        }
    }

    fn bootstrap(
        input: I,
        target_channels: ChannelCount,
        target_sample_rate: SampleRate,
    ) -> UniformInner<I> {
        let from_channels = input.channels();
        let from_sample_rate = input.sample_rate();

        let needs_rate_conversion = from_sample_rate != target_sample_rate;
        let needs_channel_conversion = from_channels != target_channels;

        match (needs_rate_conversion, needs_channel_conversion) {
            (false, false) => UniformInner::Passthrough(input),
            (true, false) => {
                let config = ResampleConfig::poly().degree(Poly::Linear).build();
                let rate_converted = Resample::new(input, target_sample_rate, config);
                UniformInner::SampleRate(rate_converted)
            }
            (false, true) => {
                let channel_converted =
                    ChannelCountConverter::new(input, from_channels, target_channels);
                UniformInner::ChannelCount(channel_converted)
            }
            (true, true) => {
                let config = ResampleConfig::poly().degree(Poly::Linear).build();

                if target_channels > from_channels {
                    let rate_converted = Resample::new(input, target_sample_rate, config);
                    let channel_converted =
                        ChannelCountConverter::new(rate_converted, from_channels, target_channels);
                    UniformInner::BothUpmix(channel_converted)
                } else {
                    let channel_converted =
                        ChannelCountConverter::new(input, from_channels, target_channels);
                    let rate_converted =
                        Resample::new(channel_converted, target_sample_rate, config);
                    UniformInner::BothDownmix(rate_converted)
                }
            }
        }
    }
}

impl<I> Iterator for UniformSourceIterator<I>
where
    I: Source,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(span_len) = self.cached_span_len {
            if self.current_span_pos >= span_len {
                // At span boundary - check if parameters changed
                let source = self.inner.as_mut().unwrap().inner_mut();
                let new_channels = source.channels();
                let new_sample_rate = source.sample_rate();

                let parameters_changed = new_channels != self.current_channels
                    || new_sample_rate != self.current_sample_rate;

                if parameters_changed {
                    let source = self.inner.take().unwrap().into_inner();
                    self.current_channels = new_channels;
                    self.current_sample_rate = new_sample_rate;
                    let new_inner = UniformSourceIterator::bootstrap(
                        source,
                        self.target_channels,
                        self.target_sample_rate,
                    );
                    self.inner = Some(new_inner);
                }

                // Calculate new output span length based on the conversion type
                let new_span_len = match self.inner.as_ref().unwrap() {
                    UniformInner::Passthrough(source) => source.current_span_len(),
                    UniformInner::SampleRate(resample) => resample.current_span_len(),
                    UniformInner::ChannelCount(converter) => converter.current_span_len(),
                    UniformInner::BothUpmix(converter) => converter.current_span_len(),
                    UniformInner::BothDownmix(converter) => converter.current_span_len(),
                };

                self.current_span_pos = 0;
                self.cached_span_len = new_span_len;
            }
        }

        if let Some(sample) = self.inner.as_mut().unwrap().next() {
            // Only increment counter when tracking spans
            if self.cached_span_len.is_some() {
                self.current_span_pos += 1;
            }
            return Some(sample);
        }

        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.inner.as_ref().unwrap().size_hint().0, None)
    }
}

impl<I> Source for UniformSourceIterator<I>
where
    I: Iterator + Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.target_channels
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.target_sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.inner.as_ref().unwrap().inner().total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        if let Some(input) = self.inner.as_mut() {
            input.inner_mut().try_seek(pos)
        } else {
            Ok(())
        }
    }
}
