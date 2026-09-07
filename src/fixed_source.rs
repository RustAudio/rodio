//! Sources of sound and various filters which never change sample rate or
//! channel count.
use std::time::Duration;

use crate::source::SeekError;
use crate::{ChannelCount, ConstSource, Sample, SampleRate};

mod buffer;
mod chain;

pub use buffer::SamplesBuffer;
pub use chain::{ParamsMismatch, SourceChain};

/// Similar to `Source`, something that can produce interleaved samples for a
/// fixed amount of channels at a fixed sample rate. Those parameters never
/// change.
pub trait FixedSource: Iterator<Item = Sample> {
    /// May NEVER return something else once its returned a value
    fn channels(&self) -> ChannelCount;
    /// May NEVER return something else once its returned a value
    fn sample_rate(&self) -> SampleRate;
    /// Returns the total duration of this source, if known.
    ///
    /// `None` indicates at the same time "infinite" or "unknown".
    fn total_duration(&self) -> Option<Duration>;

    #[allow(unused_variables)]
    #[doc = include_str!("docs/try_seek.md")]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported {
            underlying_source: std::any::type_name::<Self>(),
        })
    }

    /// Tries to convert from a fixed source to a const one assuming
    /// the parameters already match. If they do not this returns an error.
    ///
    /// If the parameters do not match you can resample using:
    /// [`with_sample_rate`](Self::placeholder) and
    /// [`with_channel_count`](Self::placeholder).
    fn try_into_const_source<const SR: u32, const CH: u16>(
        self,
    ) -> Result<IntoConstSource<SR, CH, Self>, ParameterMismatch<SR, CH>>
    where
        Self: Sized,
    {
        if self.channels().get() != CH || self.sample_rate().get() != SR {
            Err(ParameterMismatch {
                sample_rate: self.sample_rate(),
                channel_count: self.channels(),
            })
        } else {
            Ok(IntoConstSource(self))
        }
    }

    /// Use this fixed source as if it's a dynamic source. You generally do not
    /// want to do this since there are less effects for dynamic sources and
    /// those that are available can not be implemented as efficient. This is
    /// therefore purely provided for backwards compatibility.
    fn into_dynamic_source(self) -> IntoDynamicSource<Self>
    where
        Self: Sized,
    {
        IntoDynamicSource(self)
    }

    #[doc = include_str!("docs/collect_into_buffer.md")]
    fn collect_into_buffer(self) -> SamplesBuffer
    where
        Self: Sized,
    {
        SamplesBuffer::new(
            self.channels(),
            self.sample_rate(),
            self.collect::<Vec<_>>(),
        )
    }

    /// Add another source to play directly after this one.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use rodio::nz;
    /// # use rodio::FixedSource;
    /// # use rodio::fixed_source::SamplesBuffer;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let preamble = SamplesBuffer::new(nz!(1), nz!(1), [1.0, 1.0]);
    /// let signal = SamplesBuffer::new(nz!(1), nz!(1), [2.0, 2.0]);
    ///
    /// let mixed = preamble.try_chain_source(signal)?;
    /// assert_eq!(mixed.collect::<Vec<_>>(), vec![1.0,1.0,2.0,2.0]);
    /// # Ok(())
    /// # }
    /// ```
    fn try_chain_source<S: FixedSource>(
        self,
        next: S,
    ) -> Result<SourceChain<Self, S>, chain::ParamsMismatch>
    where
        Self: Sized,
    {
        SourceChain::new(self, next)
    }

    // placeholder until effects land (need this for some examples)
    #[allow(missing_docs)]
    fn take_duration(self, _duration: Duration) -> Placeholder<Self>
    where
        Self: Sized,
    {
        todo!()
    }

    /// here to make docs links work without the linked item being in
    /// remove before next release
    fn placeholder(&self) {}
}

// placeholder until effects land (need this for some examples)
#[allow(missing_docs)]
pub struct Placeholder<S>
where
    S: FixedSource,
{
    inner: std::marker::PhantomData<S>,
}

impl<S> Placeholder<S>
where
    S: FixedSource,
{
    /// placeholder
    pub fn record(self) -> Placeholder<S> {
        self
    }
}

impl<S: FixedSource> FixedSource for Placeholder<S> {
    fn channels(&self) -> ChannelCount {
        unimplemented!("placeholder")
    }
    fn sample_rate(&self) -> SampleRate {
        unimplemented!("placeholder")
    }
    fn total_duration(&self) -> Option<Duration> {
        unimplemented!("placeholder")
    }
}

impl<S: FixedSource> Iterator for Placeholder<S> {
    type Item = Sample;
    fn next(&mut self) -> Option<Self::Item> {
        unimplemented!("placeholder")
    }
}

/// A [`ConstSource`] adapted from a [`FixedSource`].
pub struct IntoConstSource<const SR: u32, const CH: u16, S: FixedSource>(S);

impl<const SR: u32, const CH: u16, S: FixedSource> ConstSource<SR, CH>
    for IntoConstSource<SR, CH, S>
{
    fn total_duration(&self) -> Option<Duration> {
        self.0.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.0.try_seek(pos)
    }
}

impl<const SR: u32, const CH: u16, S: FixedSource> Iterator for IntoConstSource<SR, CH, S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

/// Error that occurs when a [`FixedSource`] can not be converted into a
/// [`ConstSource`] with a certain sample rate and channel count.
#[derive(Debug)]
pub struct ParameterMismatch<const SR: u32, const CH: u16> {
    sample_rate: SampleRate,
    channel_count: ChannelCount,
}

impl<const SR: u32, const CH: u16> std::error::Error for ParameterMismatch<SR, CH> {}

impl<const SR: u32, const CH: u16> std::fmt::Display for ParameterMismatch<SR, CH> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.sample_rate.get() == SR && self.channel_count.get() == CH {
            unreachable!("ParameterMismatch error can only occur when params mismatch");
        } else if self.sample_rate.get() == SR && self.channel_count.get() != CH {
            f.write_fmt(format_args!("Fixed source's channel count: {}, does not match target const source's channel count: {}", self.channel_count.get(), CH))
        } else if self.sample_rate.get() != SR && self.channel_count.get() != CH {
            f.write_fmt(format_args!("Fixed source's sample rate and channel count ({}, {}) do not match target const source's sample rate and channel count ({} {})", self.sample_rate.get(), self.channel_count.get(), SR, CH))
        } else {
            f.write_fmt(format_args!("Fixed source's sample rate : {}, does not match target const source's sample rate: {}", self.sample_rate.get(), SR))
        }
    }
}

/// A [`DynamicSource`](crate::DynamicSource) adapted from a [`FixedSource`].
pub struct IntoDynamicSource<S: FixedSource>(S);

impl<S: FixedSource> crate::DynamicSource for IntoDynamicSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.0.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.0.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.0.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.0.try_seek(pos)
    }
}

impl<S: FixedSource> Iterator for IntoDynamicSource<S> {
    type Item = crate::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}
