use crate::common::{ChannelCount, SampleRate};
use crate::source::{resample::Poly, FromIter, Resample, ResampleConfig};
use crate::{Sample, Source};

/// Iterator that converts from one sample rate to another.
#[deprecated(
    since = "0.22.0",
    note = "Use `Resample` with `FromIter` (or `from_iter` function) directly"
)]
#[derive(Debug)]
#[allow(deprecated)]
pub struct SampleRateConverter<I>
where
    I: Iterator<Item = Sample>,
{
    inner: Resample<FromIter<I>>,
}

#[allow(deprecated)]
impl<I> SampleRateConverter<I>
where
    I: Iterator<Item = Sample>,
{
    /// Create new sample rate converter.
    pub fn new(input: I, from: SampleRate, to: SampleRate, channels: ChannelCount) -> Self {
        let adapter = FromIter::new(input, channels, from);
        let config = ResampleConfig::poly().degree(Poly::Linear).build();
        let inner = Resample::new(adapter, to, config);

        Self { inner }
    }

    /// Destroys this iterator and returns the underlying iterator.
    #[inline]
    pub fn into_inner(self) -> I {
        self.inner.into_inner().into_inner()
    }

    /// Get mutable access to the underlying iterator.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        self.inner.inner_mut().inner_mut()
    }

    /// Get access to the underlying iterator.
    #[inline]
    pub fn inner(&self) -> &I {
        self.inner.inner().inner()
    }
}

#[allow(deprecated)]
impl<I> Clone for SampleRateConverter<I>
where
    I: Iterator<Item = Sample> + Clone,
{
    fn clone(&self) -> Self {
        let from_iter = self.inner.inner();
        Self::new(
            from_iter.inner().clone(),
            from_iter.sample_rate(),
            self.inner.sample_rate(),
            from_iter.channels(),
        )
    }
}

#[allow(deprecated)]
impl<I> Iterator for SampleRateConverter<I>
where
    I: Iterator<Item = Sample>,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Sample> {
        self.inner.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

#[allow(deprecated)]
impl<I> ExactSizeIterator for SampleRateConverter<I> where
    I: Iterator<Item = Sample> + ExactSizeIterator
{
}

#[cfg(test)]
#[allow(deprecated)]
mod test {
    use super::SampleRateConverter;
    use crate::math::nz;
    use crate::Sample;

    /// Minimal smoke test to ensure the deprecated SampleRateConverter wrapper still works.
    /// Core resampling tests have been moved to src/source/resample.rs.
    #[test]
    fn deprecated_wrapper_works() {
        // Test basic upsampling
        let input: Vec<Sample> = vec![0.0, 0.5, 1.0, 0.5, 0.0];
        let from = nz!(1000);
        let to = nz!(2000);
        let channels = nz!(1);

        let converter = SampleRateConverter::new(input.into_iter(), from, to, channels);
        let output: Vec<_> = converter.collect();

        // Should produce approximately 2x samples (upsampling)
        assert!(
            output.len() >= 8 && output.len() <= 12,
            "Expected approximately 10 samples, got {}",
            output.len()
        );
    }
}
