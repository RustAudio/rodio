use std::any::type_name_of_val;
use std::sync::Arc;

use crate::source::SeekError;
use crate::ConstSource;
use crate::Sample;

/// Two chained sources, the one played after the other.
#[derive(Clone)]
pub struct SourceChain<const SR: u32, const CH: u16, S1, S2> {
    first: S1,
    second: S2,
    playing_inner: bool,
}

impl<const SR: u32, const CH: u16, S1: ConstSource<SR, CH>, S2: ConstSource<SR, CH>>
    SourceChain<SR, CH, S1, S2>
{
    pub(crate) fn new(s1: S1, s2: S2) -> Self {
        SourceChain {
            first: s1,
            second: s2,
            playing_inner: true,
        }
    }

    fn try_seek_inner(&mut self, pos: std::time::Duration) -> Result<(), ChainSeekError> {
        let Some(first) = self.first.total_duration() else {
            return Err(ChainSeekError::NoTotalDurationForFirst {
                ty: type_name_of_val(&self.first),
            });
        };

        if pos < first {
            self.first
                .try_seek(pos)
                .map_err(|error| ChainSeekError::FailedToSeekInFirst {
                    ty: type_name_of_val(&self.first),
                    error,
                })
        } else {
            self.second
                .try_seek(pos)
                .map_err(|error| ChainSeekError::FailedToSeekInSecond {
                    ty: type_name_of_val(&self.second),
                    error,
                })
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ChainSeekError {
    #[error("Could not get duration of first source ({ty}")]
    NoTotalDurationForFirst { ty: &'static str },
    #[error("Could not seek in first source")]
    FailedToSeekInFirst {
        ty: &'static str,
        #[source]
        error: SeekError,
    },
    #[error("Could not seek in second source")]
    FailedToSeekInSecond {
        ty: &'static str,
        #[source]
        error: SeekError,
    },
}

impl<const SR: u32, const CH: u16, S1: ConstSource<SR, CH>, S2: ConstSource<SR, CH>>
    ConstSource<SR, CH> for SourceChain<SR, CH, S1, S2>
{
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.first
            .total_duration()
            .and_then(|d| self.second.total_duration().map(|d2| d2 + d))
    }

    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), SeekError> {
        self.try_seek_inner(pos)
            .map_err(Arc::new)
            .map_err(|e| SeekError::Other(e))
    }
}

impl<const SR: u32, const CH: u16, S1: ConstSource<SR, CH>, S2: ConstSource<SR, CH>> Iterator
    for SourceChain<SR, CH, S1, S2>
{
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.playing_inner {
            match self.first.next() {
                Some(sample) => Some(sample),
                None => {
                    self.playing_inner = false;
                    self.second.next()
                }
            }
        } else {
            self.second.next()
        }
    }
}
