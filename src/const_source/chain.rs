use crate::ConstSource;
use crate::Sample;

/// Two chained sources, the one played after the other.
#[derive(Clone)]
pub struct SourceChain<const SR: u32, const CH: u16, S1, S2> {
    inner: S1,
    next: S2,
    playing_inner: bool,
}

impl<const SR: u32, const CH: u16, S1: ConstSource<SR, CH>, S2: ConstSource<SR, CH>>
    SourceChain<SR, CH, S1, S2>
{
    pub(crate) fn new(s1: S1, s2: S2) -> Self {
        SourceChain {
            inner: s1,
            next: s2,
            playing_inner: true,
        }
    }
}

impl<const SR: u32, const CH: u16, S1: ConstSource<SR, CH>, S2: ConstSource<SR, CH>>
    ConstSource<SR, CH> for SourceChain<SR, CH, S1, S2>
{
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner
            .total_duration()
            .and_then(|d| self.next.total_duration().map(|d2| d2 + d))
    }
}

impl<const SR: u32, const CH: u16, S1: ConstSource<SR, CH>, S2: ConstSource<SR, CH>> Iterator
    for SourceChain<SR, CH, S1, S2>
{
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.playing_inner {
            match self.inner.next() {
                Some(sample) => Some(sample),
                None => {
                    self.playing_inner = false;
                    self.next.next()
                }
            }
        } else {
            self.next.next()
        }
    }
}
