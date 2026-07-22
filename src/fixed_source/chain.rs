use std::fmt::Display;

use crate::FixedSource;
use crate::Sample;
use crate::{ChannelCount, SampleRate};

/// Two chained sources, the one played after the other.
#[derive(Clone)]
pub struct SourceChain<S1, S2> {
    first: S1,
    second: S2,
    playing_inner: bool,
}

#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub struct ParamsMismatch {
    sample_rate_self: SampleRate,
    channel_count_self: ChannelCount,
    sample_rate_adding: SampleRate,
    channel_count_adding: ChannelCount,
}

impl Display for ParamsMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ParamsMismatch {
            sample_rate_self,
            channel_count_self,
            sample_rate_adding,
            channel_count_adding,
        } = self;
        f.write_fmt(format_args!("Parameters mismatch the source chained onto Self does not have the same parameters. Self has sample rate: {sample_rate_self} and channel count: {channel_count_self} while the source being chained has sample rate: {sample_rate_adding} and channel count: {channel_count_adding}."))
    }
}

impl<S1: FixedSource, S2: FixedSource> SourceChain<S1, S2> {
    pub(crate) fn new(s1: S1, s2: S2) -> Result<Self, ParamsMismatch> {
        if s1.sample_rate() != s2.sample_rate() || s1.channels() != s2.channels() {
            Err(ParamsMismatch {
                sample_rate_self: s1.sample_rate(),
                channel_count_self: s1.channels(),
                sample_rate_adding: s2.sample_rate(),
                channel_count_adding: s2.channels(),
            })
        } else {
            Ok(SourceChain {
                first: s1,
                second: s2,
                playing_inner: true,
            })
        }
    }
}

pub use crate::common::source::chain::ChainSeekError;
impl<S1: FixedSource, S2: FixedSource> FixedSource for SourceChain<S1, S2> {
    crate::common::source::chain::source_impl! {}
}

impl<S1: FixedSource, S2: FixedSource> Iterator for SourceChain<S1, S2> {
    crate::common::source::chain::iter_impl! {}
}
