use std::marker::PhantomData;
use std::time::Duration;
use Sample;
use Source;

/// An empty source.
#[derive(Debug, Copy, Clone)]
pub struct Empty<S>(PhantomData<S>);

impl<S> Empty<S> {
    #[inline]
    pub fn new() -> Empty<S> {
        Empty(PhantomData)
    }
}

impl<S> Iterator for Empty<S> {
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        None
    }
}

impl<S> Source for Empty<S> where S: Sample {
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        1
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        48000
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        Some(Duration::new(0, 0))
    }
}
