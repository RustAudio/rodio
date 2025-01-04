use crate::Sample;

pub(crate) mod fast_inhouse;
// #[cfg(feature = "experimental-hifi-resampler")]
pub(crate) mod hifi_rubato;

pub trait Resampler<I, O>: Iterator<Item = O>
where
    I: Iterator,
    I::Item: Sample + Clone,
    O: Sample,
{
    type Parts;
    fn new_parts() -> Self::Parts;

    fn from_parts(
        input: I,
        parts: Self::Parts,
        from: cpal::SampleRate,
        to: cpal::SampleRate,
        num_channels: cpal::ChannelCount,
    ) -> Self;
    fn inner_mut(&mut self) -> &mut I;
    fn into_source_and_parts(self) -> (I, Self::Parts);
}

impl<I, O> Resampler<I, O> for fast_inhouse::SampleRateConverter<I, O>
where
    I: Iterator,
    I::Item: Sample + Clone,
    O: Sample + cpal::FromSample<I::Item>,
{
    type Parts = ();

    fn new_parts() -> Self::Parts {
        ()
    }

    fn from_parts(
        input: I,
        _: Self::Parts,
        from: cpal::SampleRate,
        to: cpal::SampleRate,
        num_channels: cpal::ChannelCount,
    ) -> fast_inhouse::SampleRateConverter<I, O> {
        fast_inhouse::SampleRateConverter::new(input, from, to, num_channels)
    }

    fn inner_mut(&mut self) -> &mut I {
        self.inner_mut()
    }
    fn into_source_and_parts(self) -> (I, Self::Parts) {
        (self.into_inner(), ())
    }
}

impl<I, O> Resampler<I, O> for hifi_rubato::SampleRateConverter<I, O>
where
    I: Iterator,
    I::Item: Sample + Clone,
    O: Sample,
{
    type Parts = ();

    fn new_parts() -> Self::Parts {
        ()
    }

    fn from_parts(
        input: I,
        _: Self::Parts,
        from: cpal::SampleRate,
        to: cpal::SampleRate,
        num_channels: cpal::ChannelCount,
    ) -> hifi_rubato::SampleRateConverter<I, O> {
        hifi_rubato::SampleRateConverter::new(input, from, to, num_channels)
    }

    fn inner_mut(&mut self) -> &mut I {
        self.inner_mut()
    }
    fn into_source_and_parts(self) -> (I, Self::Parts) {
        (self.into_inner(), ())
    }
}
