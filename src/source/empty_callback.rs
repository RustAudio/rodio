use std::marker::PhantomData;
use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::{Sample, Source};

/// An empty source which executes a callback function
pub struct EmptyCallback<S> {
    #[allow(missing_docs)] // See: https://github.com/RustAudio/rodio/issues/615
    pub phantom_data: PhantomData<S>,
    #[allow(missing_docs)] // See: https://github.com/RustAudio/rodio/issues/615
    pub callback: Box<dyn Send + Fn()>,
}

impl<S> EmptyCallback<S> {
    #[inline]
    /// Create an empty source which executes a callback function.
    /// Example use-case:
    ///
    /// Detect and do something when the source before this one has ended.
    pub fn new(callback: Box<dyn Send + Fn()>) -> EmptyCallback<S> {
        EmptyCallback {
            phantom_data: PhantomData,
            callback,
        }
    }
}

impl<S> Iterator for EmptyCallback<S> {
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        (self.callback)();
        None
    }
}

impl<S> Source for EmptyCallback<S>
where
    S: Sample,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        1
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        48000
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::new(0, 0))
    }

    #[inline]
    fn try_seek(&mut self, _: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported {
            underlying_source: std::any::type_name::<Self>(),
        })
    }
}
