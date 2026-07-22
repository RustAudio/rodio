use crate::source::SeekError;

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

macro_rules! source_impl {
    () => {
        fn channels(&self) -> crate::ChannelCount {
            self.first.channels()
        }

        fn sample_rate(&self) -> crate::SampleRate {
            self.first.sample_rate()
        }

        fn total_duration(&self) -> Option<std::time::Duration> {
            self.first
                .total_duration()
                .and_then(|d| self.second.total_duration().map(|d2| d2 + d))
        }

        fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), crate::source::SeekError> {
            use crate::source::SeekError;
            use std::any::type_name_of_val;
            use std::sync::Arc;

            let Some(first) = self.first.total_duration() else {
                return Err(ChainSeekError::NoTotalDurationForFirst {
                    ty: type_name_of_val(&self.first),
                })
                .map_err(Arc::new)
                .map_err(|e| SeekError::Other(e));
            };

            if pos < first {
                self.first
                    .try_seek(pos)
                    .map_err(|error| ChainSeekError::FailedToSeekInFirst {
                        ty: type_name_of_val(&self.first),
                        error,
                    })
                    .map_err(Arc::new)
                    .map_err(|e| SeekError::Other(e))
            } else {
                self.second
                    .try_seek(pos)
                    .map_err(|error| ChainSeekError::FailedToSeekInSecond {
                        ty: type_name_of_val(&self.second),
                        error,
                    })
                    .map_err(Arc::new)
                    .map_err(|e| SeekError::Other(e))
            }
        }
    };
}

macro_rules! iter_impl {
    () => {
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
    };
}

pub(crate) use iter_impl;
pub(crate) use source_impl;
