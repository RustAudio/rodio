use crate::source::SeekError;

#[derive(Debug, thiserror::Error)]
pub enum ChainSeekError {
    #[error("Could not get duration of first source ({ty})")]
    NoTotalDurationForFirst { ty: &'static str },
    #[error("Could not seek in first source ({ty})")]
    FailedToSeekInFirst {
        ty: &'static str,
        #[source]
        error: SeekError,
    },
    #[error("Could not reset first source ({ty}) to start")]
    FailedToResetFirst {
        ty: &'static str,
        #[source]
        error: SeekError,
    },
    #[error("Could not seek in second source ({ty})")]
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
                // Reset first source to prevent a jump to the current position
                // after the first source completes again.
                if !self.playing_first {
                    // FIXME(yara): implement Seekable trait for all sources and extract
                    // this to a function. (all sources are required to impl Seekable).
                    // Might wanna do a similar thing for other shared functionality
                    // like total duration
                    self.second
                        .try_seek(std::time::Duration::ZERO)
                        .map_err(|error| ChainSeekError::FailedToResetFirst {
                            ty: type_name_of_val(&self.first),
                            error,
                        })
                        .map_err(Arc::new)
                        .map_err(|e| SeekError::Other(e))?;
                }

                self.first
                    .try_seek(pos)
                    .map_err(|error| ChainSeekError::FailedToSeekInFirst {
                        ty: type_name_of_val(&self.first),
                        error,
                    })
                    .map_err(Arc::new)
                    .map_err(|e| SeekError::Other(e))?;
                self.playing_first = true;
                Ok(())
            } else {
                self.second
                    .try_seek(pos - first)
                    .map_err(|error| ChainSeekError::FailedToSeekInSecond {
                        ty: type_name_of_val(&self.second),
                        error,
                    })
                    .map_err(Arc::new)
                    .map_err(|e| SeekError::Other(e))?;
                self.playing_first = false;
                Ok(())
            }
        }
    };
}

macro_rules! iter_impl {
    () => {
        type Item = Sample;

        fn next(&mut self) -> Option<Self::Item> {
            if self.playing_first {
                match self.first.next() {
                    Some(sample) => Some(sample),
                    None => {
                        self.playing_first = false;
                        self.second.next()
                    }
                }
            } else {
                self.second.next()
            }
        }

        #[inline]
        fn size_hint(&self) -> (usize, Option<usize>) {
            let (lower_bound_a, upper_bound_a) = self.first.size_hint();
            let (lower_bound_b, upper_bound_b) = self.second.size_hint();

            let lower_bound = lower_bound_a + lower_bound_b;
            let upper_bound = upper_bound_a.zip(upper_bound_b).map(|(a, b)| a + b);
            (lower_bound, upper_bound)
        }
    };
}

pub(crate) use iter_impl;
pub(crate) use source_impl;
