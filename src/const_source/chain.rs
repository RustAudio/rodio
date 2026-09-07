use crate::ConstSource;
use crate::Sample;

/// Two chained sources, the one played after the other.
#[derive(Clone)]
pub struct SourceChain<const SR: u32, const CH: u16, S1, S2> {
    first: S1,
    second: S2,
    playing_first: bool,
}

impl<const SR: u32, const CH: u16, S1: ConstSource<SR, CH>, S2: ConstSource<SR, CH>>
    SourceChain<SR, CH, S1, S2>
{
    pub(crate) fn new(s1: S1, s2: S2) -> Self {
        SourceChain {
            first: s1,
            second: s2,
            playing_first: true,
        }
    }
}

// FIXME(yara) extract all this into yet another macro? macro's all the way down time :3
pub use crate::common::source::chain::ChainSeekError;
impl<const SR: u32, const CH: u16, S1: ConstSource<SR, CH>, S2: ConstSource<SR, CH>>
    ConstSource<SR, CH> for SourceChain<SR, CH, S1, S2>
{
    crate::common::source::chain::source_impl! {}
}

impl<const SR: u32, const CH: u16, S1: ConstSource<SR, CH>, S2: ConstSource<SR, CH>> Iterator
    for SourceChain<SR, CH, S1, S2>
{
    crate::common::source::chain::iter_impl! {}
}

impl<const SR: u32, const CH: u16, S1: ConstSource<SR, CH>, S2: ConstSource<SR, CH>>
    ExactSizeIterator for SourceChain<SR, CH, S1, S2>
{
}
