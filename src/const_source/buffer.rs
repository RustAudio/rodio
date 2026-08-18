use std::sync::Arc;
use std::time::Duration;

use crate::source::SeekError;
use crate::ConstSource;
use crate::Sample;

/// A buffer of samples treated as a source.
#[derive(Debug, Clone)]
pub struct SamplesBuffer<const SR: u32, const CH: u16> {
    data: Arc<[Sample]>,
    pos: usize,
}

impl<const SR: u32, const CH: u16> SamplesBuffer<SR, CH> {
    /// Builds a new `SamplesBuffer`.
    ///
    /// Note any call to total_duration will panic if the buffer is larger then
    /// 16 billion elements.
    pub fn new<D>(data: D) -> SamplesBuffer<SR, CH>
    where
        D: Into<Vec<Sample>>,
    {
        const { assert!(SR > 0) };
        const { assert!(CH > 0) };

        SamplesBuffer {
            data: data.into().into(),
            pos: 0,
        }
    }
}

impl<const SR: u32, const CH: u16> ConstSource<SR, CH> for SamplesBuffer<SR, CH> {
    crate::common::source::buffer::source_impl! {}
}

impl<const SR: u32, const CH: u16> Iterator for SamplesBuffer<SR, CH> {
    crate::common::source::buffer::iter_impl! {}
}
