//! A sound source that has it's sample rate and number of channels fixed at compile time.
//! Some practical examples:
//! - An effect performed by a neural network trained on a specific channel
//!   count and sample rate
//! - A custom audio source streaming in at a pre-determined fixed sample rate.
//!   We use this in our VoIP example.
//!
//! A const pipeline _may_ also be optimized more by the compiler as many
//! branches are known at compile time, like when converting from one channel
//! count to another.
use std::num::NonZeroU16;
use std::num::NonZeroU32;
use std::time::Duration;

use crate::source::SeekError;
use crate::ChannelCount;
use crate::FixedSource;
use crate::Sample;
use crate::SampleRate;
use crate::Source as DynamicSource; // Source will (probably) be renamed to this later

mod buffer;
mod chain;
mod conversions;

pub use buffer::SamplesBuffer;
pub use chain::SourceChain;
pub use conversions::channel_count::ChannelConvertor;
pub use conversions::sample_rate::SampleRateConvertor;

/// A source which sample rate and channel count are fixed at compile time.
pub trait ConstSource<const SR: u32, const CH: u16>: Iterator<Item = Sample> {
    /// Returns the sample rate which is the first generic (SR) of a ConstSource
    fn sample_rate(&self) -> SampleRate {
        const { NonZeroU32::new(SR).expect("SampleRate must be > 0") }
    }
    /// Returns the channel count which is the second generic (CH) of a ConstSource
    fn channels(&self) -> ChannelCount {
        const { NonZeroU16::new(CH).expect("Channel count must be > 0") }
    }

    #[doc = include_str!("docs/total_duration.md")]
    fn total_duration(&self) -> Option<Duration>;

    #[allow(unused_variables)]
    #[doc = include_str!("docs/try_seek.md")]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported {
            underlying_source: std::any::type_name::<Self>(),
        })
    }

    /// Convert from `SR` (the current sample rate) to `SR_OUT`.
    ///
    /// Though the defaults cover most use-cases you can configure
    /// the resampler using [`with_config`](SampleRateConvertor::with_config).
    fn with_sample_rate<const SR_OUT: u32>(self) -> SampleRateConvertor<SR, SR_OUT, CH, Self>
    where
        Self: Sized,
    {
        SampleRateConvertor::new(self)
    }

    /// Convert from the current channel count to `CH_OUT`.
    fn with_channel_count<const CH_OUT: u16>(self) -> ChannelConvertor<SR, CH, CH_OUT, Self>
    where
        Self: Sized,
    {
        ChannelConvertor::new(self)
    }

    /// Use this const source as if it's a dynamic source. You generally do not
    /// want to do this since there are less effects for dynamic sources and
    /// those that are available can not be implemented as efficient. This is
    /// therefore purely provided for backwards compatibility.
    fn into_dynamic_source(self) -> IntoDynamicSource<SR, CH, Self>
    where
        Self: Sized,
    {
        IntoDynamicSource { inner: self }
    }

    /// Use this const source as if it's a fixed source which is generally
    /// easier to work with since it drops the generics. The same effects are
    /// available for both.
    ///
    /// # Example
    ///
    /// ```rust
    /// # struct CustomEffect<S: FixedSource>(S);
    /// # use rodio::{FixedSource, ConstSource};
    /// # use rodio::generators::const_source;
    ///
    /// // Note custom effect can only wrap a FixedSource
    /// fn apply_custom_effect<S: FixedSource>(source: S) -> CustomEffect<S> {
    ///     CustomEffect(source)
    /// }
    ///
    /// let source = const_source::Silence::<44100>::new();
    /// let source = source.into_fixed_source();
    /// apply_custom_effect(source);
    /// ```
    fn into_fixed_source(self) -> IntoFixedSource<SR, CH, Self>
    where
        Self: Sized,
    {
        IntoFixedSource { inner: self }
    }

    #[doc = include_str!("docs/collect_into_buffer.md")]
    fn collect_into_buffer(self) -> SamplesBuffer<SR, CH>
    where
        Self: Sized,
    {
        SamplesBuffer::new(self.collect::<Vec<_>>())
    }

    /// Add another source to play directly after this one.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use rodio::const_source::ConstSource;
    /// # use rodio::const_source::SamplesBuffer;
    /// let preamble = SamplesBuffer::<44100, 1>::new([1.0, 1.0]);
    /// let signal = SamplesBuffer::<44100, 1>::new([2.0, 2.0]);
    ///
    /// let mixed = preamble.chain_source(signal);
    /// assert_eq!(mixed.collect::<Vec<_>>(), vec![1.0,1.0,2.0,2.0])
    /// ```
    fn chain_source<S>(self, next: S) -> SourceChain<SR, CH, Self, S>
    where
        Self: Sized,
        S: ConstSource<SR, CH>,
    {
        SourceChain::new(self, next)
    }

    // placeholder until effects land (need this for some examples)
    #[allow(missing_docs)]
    fn take_duration(self, _duration: Duration) -> Placeholder<SR, CH, Self>
    where
        Self: Sized,
    {
        todo!()
    }
}

// placeholder until effects land (need this for some examples)
#[allow(missing_docs)]
pub struct Placeholder<const SR: u32, const CH: u16, S>
where
    S: ConstSource<SR, CH>,
{
    inner: std::marker::PhantomData<S>,
}

/// A `DynamicSource` converted from a `ConstSource`. Useful for passing to old
/// APIs that do not accept a `ConstSource` nor `FixedSource`.
#[derive(Clone)]
pub struct IntoDynamicSource<const SR: u32, const CH: u16, S>
where
    S: ConstSource<SR, CH>,
{
    inner: S,
}

impl<const SR: u32, const CH: u16, S> Iterator for IntoDynamicSource<SR, CH, S>
where
    S: ConstSource<SR, CH>,
{
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<const SR: u32, const CH: u16, S> DynamicSource for IntoDynamicSource<SR, CH, S>
where
    S: ConstSource<SR, CH>,
{
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        const { NonZeroU16::new(CH).expect("channel count must be larger then zero") }
    }

    fn sample_rate(&self) -> SampleRate {
        const { NonZeroU32::new(SR).expect("sample rate must be larger then zero") }
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }
}

/// A `FixedSource` converted from a `ConstSource`. Useful for passing to APIs
/// that do not accept a `ConstSource`.
#[derive(Clone)]
pub struct IntoFixedSource<const SR: u32, const CH: u16, S>
where
    S: ConstSource<SR, CH>,
{
    inner: S,
}

impl<const SR: u32, const CH: u16, S> Iterator for IntoFixedSource<SR, CH, S>
where
    S: ConstSource<SR, CH>,
{
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<const SR: u32, const CH: u16, S> FixedSource for IntoFixedSource<SR, CH, S>
where
    S: ConstSource<SR, CH>,
{
    fn channels(&self) -> ChannelCount {
        const { NonZeroU16::new(CH).expect("channel count must be larger then zero") }
    }

    fn sample_rate(&self) -> SampleRate {
        const { NonZeroU32::new(SR).expect("sample rate must be larger then zero") }
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}
