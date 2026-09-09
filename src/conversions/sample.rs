use dasp_sample::{FromSample, ToSample};
use std::marker::PhantomData;

/// Converts each sample's numeric type to `O`.
///
/// Rescales the values to the target type's range (for example `i16` to `f32`), leaving the
/// channel count and sample rate unchanged.
#[derive(Clone, Debug)]
pub struct SampleTypeConverter<I, O> {
    input: I,
    marker: PhantomData<O>,
}

impl<S, O> SampleTypeConverter<S, O> {
    /// Builds a new converter.
    #[inline]
    pub fn new(input: S) -> SampleTypeConverter<S, O> {
        SampleTypeConverter {
            input,
            marker: PhantomData,
        }
    }

    crate::common::source::add_inner_accessors! {input}
}

impl<I, O> Iterator for SampleTypeConverter<I, O>
where
    I: Iterator,
    I::Item: ToSample<O>,
{
    type Item = O;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.input.next().map(|s| s.to_sample_())
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I, O> ExactSizeIterator for SampleTypeConverter<I, O>
where
    I: ExactSizeIterator,
    O: FromSample<I::Item>,
{
}
