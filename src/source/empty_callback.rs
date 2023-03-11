use std::marker::PhantomData;
use std::time::Duration;

use crate::{Sample, Source};

/// An empty source which executes a callback function
pub struct EmptyCallback<S> {
    pub phantom_data: PhantomData<S>,
    pub callback: Box<dyn Send + Fn()>,
}

impl<S> EmptyCallback<S> {
    #[inline]
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
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> u16 {
        1
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        48000
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::new(0, 0))
    }
}
