use crate::conversions::sample_rate::rubato::{ResampleInner, RubatoAsyncResample};
use crate::conversions::Interpolation;
use crate::math::gcd;
use crate::source::ResampleConfig;
use crate::{ConstSource, Sample, SampleRate, Source};

use crate::const_source::IntoDynamicSource;
#[cfg(feature = "rubato-fft")]
use crate::conversions::sample_rate::rubato::RubatoFftResample;
use crate::conversions::sample_rate::{InSamples, OutSamples};

/// Resamples an audio source to a target sample rate using Rubato.
pub struct SampleRateConvertor<
    const SR_IN: u32,
    const SR_OUT: u32,
    const CH: u16,
    S: ConstSource<SR_IN, CH>,
> {
    // Option so we can take out the source and rebuild the resampler without unsafe
    inner: Option<ResampleInner<IntoDynamicSource<SR_IN, CH, S>>>,
}

#[derive(thiserror::Error)]
#[error("The resampler was already running")]
pub struct ResamplerRunning<
    const SR_IN: u32,
    const SR_OUT: u32,
    const CH: u16,
    S: ConstSource<SR_IN, CH>,
>(SampleRateConvertor<SR_IN, SR_OUT, CH, S>);

impl<const SR_IN: u32, const SR_OUT: u32, const CH: u16, S: ConstSource<SR_IN, CH>>
    SampleRateConvertor<SR_IN, SR_OUT, CH, S>
{
    pub(crate) fn new(source: S) -> Self {
        Self {
            inner: Some(Self::create_resampler(
                source.into_dynamic_source(),
                ResampleConfig::default(),
            )),
        }
    }

    /// Further configure the resampler created with [`with_sample_rate`](ConstSource::with_sample_rate).
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
    /// # use rodio::generators::const_source::Silence;
    /// # use rodio::SampleRate;
    /// # fn hi() -> Option<()> { // to enable ? in the example
    /// use rodio::ConstSource;
    /// use rodio::conversions::ResampleConfig;
    ///
    /// let source: Silence<44100> = Silence::new();
    /// let resampled = source
    ///     .with_sample_rate::<48000>()
    ///     .with_config(ResampleConfig::fast());
    /// # Some(())
    /// # }
    /// # hi().unwrap();
    /// ```
    #[allow(clippy::result_large_err, reason = "the Ok variant is the same size")]
    pub fn with_config(
        mut self,
        config: ResampleConfig,
    ) -> Result<Self, ResamplerRunning<SR_IN, SR_OUT, CH, S>> {
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
            inner: Some(Self::create_resampler(source, config)),
        })
    }

    fn resampler(&self) -> &ResampleInner<IntoDynamicSource<SR_IN, CH, S>> {
        self.inner
            .as_ref()
            .expect("never none outside `with_config`")
    }

    fn resampler_mut(&mut self) -> &mut ResampleInner<IntoDynamicSource<SR_IN, CH, S>> {
        self.inner
            .as_mut()
            .expect("never none outside `with_config`")
    }

    fn create_resampler(
        source: IntoDynamicSource<SR_IN, CH, S>,
        config: ResampleConfig,
    ) -> ResampleInner<IntoDynamicSource<SR_IN, CH, S>> {
        let source_rate = const { SampleRate::new(SR_IN).expect("checked in 'new'") };
        if SR_IN == SR_OUT {
            let channels = source.channels();
            ResampleInner::Passthrough {
                source_rate,
                source,
                input_span_pos: InSamples::ZERO,
                channels,
            }
        } else {
            let target_rate = const { SampleRate::new(SR_OUT).expect("checked in 'new'") };
            match config {
                ResampleConfig::Poly { degree, chunk_size } => {
                    let resampler =
                        RubatoAsyncResample::new_poly(source, target_rate, chunk_size, degree)
                            .expect("Failed to create polynomial resampler");
                    ResampleInner::Poly(resampler)
                }
                ResampleConfig::Sinc(mut sinc) => {
                    #[cfg(feature = "rubato-fft")]
                    if sinc.is_supported_fixed_ratio(target_rate, source_rate) {
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

impl<const SR_IN: u32, const SR_OUT: u32, const CH: u16, S: ConstSource<SR_IN, CH>>
    ConstSource<SR_OUT, CH> for SampleRateConvertor<SR_IN, SR_OUT, CH, S>
{
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.resampler().inner().total_duration()
    }
}

impl<const SR_IN: u32, const SR_OUT: u32, const CH: u16, S> Iterator
    for SampleRateConvertor<SR_IN, SR_OUT, CH, S>
where
    S: ConstSource<SR_IN, CH>,
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
