use crate::common::{ChannelCount, SampleRate};
use crate::source::{resample::Poly, Resample, ResampleConfig, SeekError};
use crate::{Sample, Source};
use std::time::Duration;

/// Iterator that converts from one sample rate to another.
///
/// Uses `Resample` internally configured with linear polynomial interpolation. This is fast but
/// low quality. For better quality, consider using `Resample` directly with a higher-quality
/// configuration.
#[derive(Debug)]
pub struct SampleRateConverter<I>
where
    I: Iterator<Item = Sample>,
{
    inner: Resample<SourceAdapter<I>>,
}

impl<I> SampleRateConverter<I>
where
    I: Iterator<Item = Sample>,
{
    /// Create new sample rate converter.
    pub fn new(input: I, from: SampleRate, to: SampleRate, channels: ChannelCount) -> Self {
        let adapter = SourceAdapter {
            iter: input,
            channels,
            sample_rate: from,
        };

        let config = ResampleConfig::poly().degree(Poly::Linear).build();
        let inner = Resample::new(adapter, to, config);

        Self { inner }
    }

    /// Destroys this iterator and returns the underlying iterator.
    #[inline]
    pub fn into_inner(self) -> I {
        self.inner.into_inner().iter
    }

    /// Get mutable access to the underlying iterator.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.inner.inner_mut().iter
    }

    /// Get access to the underlying iterator.
    #[inline]
    pub fn inner(&self) -> &I {
        &self.inner.inner().iter
    }
}

impl<I> Clone for SampleRateConverter<I>
where
    I: Iterator<Item = Sample> + Clone,
{
    fn clone(&self) -> Self {
        Self::new(
            self.inner.inner().iter.clone(),
            self.inner.inner().sample_rate(),
            self.inner.sample_rate(),
            self.inner.inner().channels(),
        )
    }
}

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

impl<I> ExactSizeIterator for SampleRateConverter<I> where
    I: Iterator<Item = Sample> + ExactSizeIterator
{
}

/// Simple adapter that provides Source trait for any iterator.
#[derive(Clone, Debug)]
struct SourceAdapter<I> {
    iter: I,
    channels: ChannelCount,
    sample_rate: SampleRate,
}

impl<I> Iterator for SourceAdapter<I>
where
    I: Iterator<Item = Sample>,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<I> Source for SourceAdapter<I>
where
    I: Iterator<Item = Sample>,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.iter.size_hint().1
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.channels
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    #[inline]
    fn try_seek(&mut self, _pos: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported {
            underlying_source: std::any::type_name::<Self>(),
        })
    }
}

impl<I> ExactSizeIterator for SourceAdapter<I> where I: ExactSizeIterator<Item = Sample> {}

#[cfg(test)]
mod test {
    use super::SampleRateConverter;
    use crate::common::{ChannelCount, SampleRate};
    use crate::math::nz;
    use crate::{Float, Sample};
    use dasp_sample::ToSample;
    use quickcheck::{quickcheck, TestResult};
    use std::num::NonZero;
    use std::time::Duration;

    /// Convert and truncate input to contain a frame-aligned number of samples.
    fn convert_to_frames<S: dasp_sample::Sample + ToSample<crate::Sample>>(
        input: Vec<S>,
        channels: ChannelCount,
    ) -> Vec<Sample> {
        let mut input: Vec<Sample> = input.iter().map(|x| x.to_sample()).collect();
        let frame_size = channels.get() as usize;
        input.truncate(frame_size * (input.len() / frame_size));
        input
    }

    quickcheck! {
        /// Check that resampling an empty input produces no output.
        fn empty(from: SampleRate, to: SampleRate, channels: ChannelCount) -> TestResult {
            if channels.get() > 128 {
                return TestResult::discard();
            }

            let input = vec![];
            let output = SampleRateConverter::new(input.clone().into_iter(), from, to, channels)
                .collect::<Vec<_>>();

            TestResult::from_bool(input == output)
        }

        /// Check that resampling to the same rate does not change the signal.
        fn identity(from: SampleRate, channels: ChannelCount, input: Vec<i16>) -> TestResult {
            if channels.get() > 128 { return TestResult::discard(); }

            let input = convert_to_frames(input, channels);
            let output = SampleRateConverter::new(input.clone().into_iter(), from, from, channels)
                .collect::<Vec<_>>();

            TestResult::from_bool(input == output)
        }

        /// Check that dividing the sample rate by k (integer) is the same as dropping a sample
        /// from each channel.
        fn divide_sample_rate(to: SampleRate, k: NonZero<u16>, input: Vec<i16>, channels: ChannelCount) -> TestResult {
            if channels.get() > 128 || to.get() > 48000 {
                return TestResult::discard();
            }

            let from = SampleRate::new(to.get() * k.get() as u32).unwrap();

            let input = convert_to_frames(input, channels);
            let output = SampleRateConverter::new(input.clone().into_iter(), from, to, channels)
                .collect::<Vec<_>>();

            let expected = input
                .chunks_exact(channels.get() as usize)
                .step_by(k.get() as usize)
                .flatten()
                .copied()
                .collect::<Vec<_>>();

            TestResult::from_bool(output == expected)
        }

        /// Check that, after multiplying the sample rate by k, every k-th sample in the output
        /// matches exactly with the input.
        fn multiply_sample_rate(from: SampleRate, k: NonZero<u16>, input: Vec<i16>, channels: ChannelCount) -> TestResult {
            if from.get() > u16::MAX as u32 || channels.get() > 128 {
                return TestResult::discard();
            }

            let to = SampleRate::new(from.get() * k.get() as u32).unwrap();

            let input = convert_to_frames(input, channels);
            let output = SampleRateConverter::new(input.clone().into_iter(), from, to, channels)
                .collect::<Vec<_>>();

            let recovered = output
                .chunks_exact(channels.get() as usize)
                .step_by(k.get() as usize)
                .flatten()
                .copied()
                .collect::<Vec<_>>();

            TestResult::from_bool(input == recovered)
        }

        /// Check that resampling does not change the audio duration, except by a negligible
        /// amount (± 1ms). Reproduces #316.
        fn preserve_durations(d: Duration, freq: f32, to: SampleRate) -> TestResult {
            use crate::source::{SineWave, Source};
            if !freq.is_normal() || freq <= 0.0 || d > Duration::from_secs(1) {
                return TestResult::discard();
            }

            let source = SineWave::new(freq).take_duration(d);
            let from = source.sample_rate();

            let resampled =
                SampleRateConverter::new(source, from, to, nz!(1));
            let duration =
                Duration::from_secs_f32(resampled.count() as f32 / to.get() as f32);

            let delta = duration.abs_diff(d);
            TestResult::from_bool(delta < Duration::from_millis(1))
        }
    }

    fn test_sample_rate_conversion(
        input: Vec<Sample>,
        from: SampleRate,
        to: SampleRate,
        channels: ChannelCount,
    ) {
        let input_len = input.len();
        let converter = SampleRateConverter::new(input.into_iter(), from, to, channels);
        let converter_len = converter.len();
        let output_len = converter.count();

        assert_eq!(
            converter_len, output_len,
            "size_hint should match actual output"
        );
        assert_eq!(
            output_len,
            (input_len as Float * to.get() as Float / from.get() as Float).ceil() as usize,
            "duration must be preserved"
        );
    }

    #[test]
    fn upsample_fractional_ratio() {
        let from = nz!(2000);
        let to = nz!(3000);
        assert!(to.get() % from.get() != 0, "should be fractional ratio");

        // 4 input frames (8 samples) at 1.5x = 6 output frames (12 samples)
        // Preserves duration: 4/2000 = 6/3000 = 0.002 seconds
        test_sample_rate_conversion(
            vec![2.0, 16.0, 4.0, 18.0, 6.0, 20.0, 8.0, 22.0],
            from,
            to,
            nz!(2),
        );
    }

    #[test]
    fn upsample_integer_ratio() {
        let from = nz!(1000);
        let to = nz!(7000);
        assert!(to.get() % from.get() == 0, "should be integer ratio");

        test_sample_rate_conversion(vec![1.0, 14.0], from, to, nz!(1));
    }

    #[test]
    fn downsample() {
        let from = nz!(12000);
        let to = nz!(2400);
        assert!(from.get() > to.get(), "should be downsampling");

        // Note: Rubato's polynomial downsampler has inherent phase offset
        // (samples at positions [4, 9, 14] instead of [0, 5, 10, 15])
        test_sample_rate_conversion(
            Vec::from_iter((0..17).map(|x| x as Sample)),
            from,
            to,
            nz!(1),
        );
    }
}
