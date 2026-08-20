use crate::conversions::Interpolation;
use crate::math::gcd;
use crate::source::ResampleConfig;
use crate::{FixedSource, Sample, SampleRate, Source};

#[cfg(feature = "rubato-fft")]
use crate::conversions::sample_rate::rubato::RubatoFftResample;
use crate::conversions::sample_rate::rubato::{ResampleInner, RubatoAsyncResample};

use crate::conversions::sample_rate::{InSamples, OutSamples};
use crate::fixed_source::IntoDynamicSource;

/// Resamples an audio source to a target sample rate using Rubato.
pub struct SampleRateConvertor<S: FixedSource> {
    // Option so we can take out the source and rebuild the resampler without unsafe
    inner: Option<ResampleInner<IntoDynamicSource<S>>>,
    target_rate: SampleRate,
}

#[derive(thiserror::Error)]
#[error("The resampler was already running")]
pub struct ResamplerRunning<S: FixedSource>(SampleRateConvertor<S>);

impl<S: FixedSource> SampleRateConvertor<S> {
    pub(crate) fn new(source: S, target_rate: SampleRate) -> Self {
        Self {
            inner: Some(Self::create_resampler(
                source.into_dynamic_source(),
                target_rate,
                ResampleConfig::default(),
            )),
            target_rate,
        }
    }

    /// Further configure the resampler created with [`with_sample_rate`](FixedSource::with_sample_rate).
    /// You usually do not need to do this unless you have a specific use-case that requires very
    /// fast or extremely high quality resampling. The [`ResampleConfig`] has a number of factories
    /// with good defaults for such use-cases.
    ///
    /// # Errors
    /// If the resampler can no longer be reconfigured, usually after yielding
    /// the first sample.
    ///
    /// # Example
    /// ```
    /// # use rodio::generators::fixed_source::Silence;
    /// # use rodio::SampleRate;
    /// # fn hi() -> Option<()> { // to enable ? in the example
    /// use rodio::FixedSource;
    /// use rodio::conversions::ResampleConfig;
    ///
    /// let source = Silence::new(SampleRate::new(44_100)?);
    /// let resampled = source
    ///     .with_sample_rate(SampleRate::new(48_000)?)
    ///     .with_config(ResampleConfig::fast());
    /// # Some(())
    /// # }
    /// # hi().unwrap();
    /// ```
    #[allow(clippy::result_large_err, reason = "the Ok variant is the same size")]
    pub fn with_config(mut self, config: ResampleConfig) -> Result<Self, ResamplerRunning<S>> {
        if !self.resampler().can_reconfigure() {
            return Err(ResamplerRunning(self));
        }

        let source = self
            .inner
            .take()
            .expect(
                "we are the only ones who set this to none and we set it to \
            some at the end of this fn",
            )
            .into_inner();

        Ok(Self {
            inner: Some(Self::create_resampler(source, self.target_rate, config)),
            target_rate: self.target_rate,
        })
    }

    fn resampler(&self) -> &ResampleInner<IntoDynamicSource<S>> {
        self.inner
            .as_ref()
            .expect("never none outside `with_config`")
    }

    fn resampler_mut(&mut self) -> &mut ResampleInner<IntoDynamicSource<S>> {
        self.inner
            .as_mut()
            .expect("never none outside `with_config`")
    }

    fn create_resampler(
        source: IntoDynamicSource<S>,
        target_rate: SampleRate,
        config: ResampleConfig,
    ) -> ResampleInner<IntoDynamicSource<S>> {
        if source.sample_rate() == target_rate {
            let channels = source.channels();
            ResampleInner::Passthrough {
                source_rate: source.sample_rate(),
                source,
                input_span_pos: InSamples::ZERO,
                channels,
            }
        } else {
            match config {
                ResampleConfig::Poly { degree, chunk_size } => {
                    let resampler =
                        RubatoAsyncResample::new_poly(source, target_rate, chunk_size, degree)
                            .expect("Failed to create polynomial resampler");
                    ResampleInner::Poly(resampler)
                }
                ResampleConfig::Sinc(mut sinc) => {
                    #[cfg(feature = "rubato-fft")]
                    if sinc.is_supported_fixed_ratio(target_rate, source.sample_rate()) {
                        let resampler = RubatoFftResample::new(
                            source,
                            target_rate,
                            sinc.chunk_size,
                            sinc.sub_chunks,
                        )
                        .expect("Failed to create FFT resampler");
                        return ResampleInner::Fft(resampler);
                    }

                    if sinc.is_supported_fixed_ratio(target_rate, source.sample_rate()) {
                        sinc.interpolation = Interpolation::Nearest;
                        let g = gcd(target_rate.get(), source.sample_rate().get());
                        let numer = target_rate.get() / g;
                        let denom = source.sample_rate().get() / g;
                        let ratio = numer.max(denom) as usize;
                        sinc.oversampling_factor = ratio;
                    }
                    ResampleInner::Sinc(sinc.build(source, target_rate))
                }
            }
        }
    }
}

impl<S: FixedSource> FixedSource for SampleRateConvertor<S> {
    fn channels(&self) -> crate::ChannelCount {
        self.resampler().inner().channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.target_rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        self.resampler().inner().total_duration()
    }
}

impl<S> Iterator for SampleRateConvertor<S>
where
    S: FixedSource,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self.resampler_mut() {
            ResampleInner::Passthrough { source, .. } => source.next(),
            ResampleInner::Poly(resampler) => resampler.next_sample(),
            ResampleInner::Sinc(resampler) => resampler.next_sample(),
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => resampler.next_sample(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.resampler() {
            ResampleInner::Passthrough { source, .. } => source.size_hint(),
            ResampleInner::Poly(resampler) | ResampleInner::Sinc(resampler) => {
                let adjusted_for_resampling = |samples| {
                    InSamples(samples).resampled_by(resampler.resample_ratio)
                        + resampler.output.len()
                        + resampler
                            .frames_being_resampled
                            .samples(resampler.output.channels)
                };
                let (lower, upper) = resampler.input.size_hint();
                let lower = adjusted_for_resampling(lower);
                let upper = upper.map(adjusted_for_resampling);
                (lower.raw(), upper.as_ref().map(OutSamples::raw))
            }
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => {
                let adjusted_for_resampling = |samples| {
                    InSamples(samples).resampled_by(resampler.resample_ratio)
                        + resampler.output.len()
                        + resampler
                            .frames_being_resampled
                            .samples(resampler.output.channels)
                };
                let (lower, upper) = resampler.input.size_hint();
                let lower = adjusted_for_resampling(lower);
                let upper = upper.map(adjusted_for_resampling);
                (lower.raw(), upper.as_ref().map(OutSamples::raw))
            }
        }
    }
}
