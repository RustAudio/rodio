use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::conversions::ChannelCountConverter;
use crate::conversions::{Poly, ResampleConfig, SampleRateConverter};
use crate::Source;

#[derive(Clone)]
enum UniformInner<I: Source> {
    Passthrough(I),
    SampleRate(SampleRateConverter<I>),
    ChannelCount(ChannelCountConverter<I>),
    // Resampled first even when downmixing: a resampler reads ahead,
    // and only on its raw input can it see where a span changes format.
    Both(ChannelCountConverter<SampleRateConverter<I>>),
}

impl<I: Source> Iterator for UniformInner<I> {
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            UniformInner::Passthrough(take) => take.next(),
            UniformInner::SampleRate(converter) => converter.next(),
            UniformInner::ChannelCount(converter) => converter.next(),
            UniformInner::Both(converter) => converter.next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            UniformInner::Passthrough(take) => take.size_hint(),
            UniformInner::SampleRate(converter) => converter.size_hint(),
            UniformInner::ChannelCount(converter) => converter.size_hint(),
            UniformInner::Both(converter) => converter.size_hint(),
        }
    }
}

impl<I: Source> UniformInner<I> {
    #[inline]
    fn into_inner(self) -> I {
        match self {
            UniformInner::Passthrough(source) => source,
            UniformInner::SampleRate(converter) => converter.into_inner(),
            UniformInner::ChannelCount(converter) => converter.into_inner(),
            UniformInner::Both(converter) => converter.into_inner().into_inner(),
        }
    }

    #[inline]
    fn inner(&self) -> &I {
        match self {
            UniformInner::Passthrough(source) => source,
            UniformInner::SampleRate(converter) => converter.inner(),
            UniformInner::ChannelCount(converter) => converter.inner(),
            UniformInner::Both(converter) => converter.inner().inner(),
        }
    }

    /// The format of the input the next sample comes from. A resampler
    /// has read ahead of it, so its input may already be further along.
    #[inline]
    fn next_input_format(&self) -> (ChannelCount, SampleRate) {
        match self {
            UniformInner::Passthrough(source) => (source.channels(), source.sample_rate()),
            UniformInner::ChannelCount(converter) => (
                converter.inner().channels(),
                converter.inner().sample_rate(),
            ),
            UniformInner::SampleRate(converter) => converter.next_input_format(),
            UniformInner::Both(converter) => converter.inner().next_input_format(),
        }
    }

    /// Whether the next sample starts an output frame. Mid-frame, the
    /// channel converter still owes samples of input it has read, so
    /// rebuilding there would drop them and shift every later channel.
    #[inline]
    fn at_frame_start(&self) -> bool {
        match self {
            UniformInner::Passthrough(_) | UniformInner::SampleRate(_) => true,
            UniformInner::ChannelCount(converter) => converter.at_frame_start(),
            UniformInner::Both(converter) => converter.at_frame_start(),
        }
    }

    #[inline]
    fn inner_mut(&mut self) -> &mut I {
        match self {
            UniformInner::Passthrough(source) => source,
            UniformInner::SampleRate(converter) => converter.inner_mut(),
            UniformInner::ChannelCount(converter) => converter.inner_mut(),
            UniformInner::Both(converter) => converter.inner_mut().inner_mut(),
        }
    }
}

impl<I: Source> Source for UniformInner<I> {
    /// The span of the converted output, which is what gets counted.
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        match self {
            UniformInner::Passthrough(source) => source.current_span_len(),
            UniformInner::SampleRate(converter) => converter.current_span_len(),
            UniformInner::ChannelCount(converter) => converter.current_span_len(),
            UniformInner::Both(converter) => converter.current_span_len(),
        }
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.inner().channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.inner().sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.inner().total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner_mut().try_seek(pos)
    }
}

/// An iterator that reads from a `Source` and converts the samples to a
/// specific type, sample-rate and channels count.
///
/// It implements `Source` as well, but all the data is guaranteed to be in a
/// single span whose channels and samples rate have been passed to `new`.
#[derive(Clone)]
pub struct UniformSourceIterator<I>
where
    I: Source,
{
    inner: Option<UniformInner<I>>,
    target_channels: ChannelCount,
    target_sample_rate: SampleRate,
    current_channels: ChannelCount,
    current_sample_rate: SampleRate,
    current_span_pos: usize,
    cached_span_len: Option<usize>,
}

impl<I> UniformSourceIterator<I>
where
    I: Source,
{
    /// Wrap a `Source` and lazily convert its samples to a specific type,
    /// sample-rate and channels count.
    pub fn new(
        input: I,
        target_channels: ChannelCount,
        target_sample_rate: SampleRate,
    ) -> UniformSourceIterator<I> {
        let current_channels = input.channels();
        let current_sample_rate = input.sample_rate();
        let inner = UniformSourceIterator::bootstrap(input, target_channels, target_sample_rate);

        Self {
            inner: Some(inner),
            target_channels,
            target_sample_rate,
            current_channels,
            current_sample_rate,
            current_span_pos: 0,
            // Check on the first pull too: the format may have changed
            // since, like a queue that was empty when added to a mixer.
            cached_span_len: Some(0),
        }
    }

    fn bootstrap(
        input: I,
        target_channels: ChannelCount,
        target_sample_rate: SampleRate,
    ) -> UniformInner<I> {
        let from_channels = input.channels();
        let from_sample_rate = input.sample_rate();

        let needs_rate_conversion = from_sample_rate != target_sample_rate;
        let needs_channel_conversion = from_channels != target_channels;
        let config = ResampleConfig::poly().degree(Poly::Linear).build();

        match (needs_rate_conversion, needs_channel_conversion) {
            (false, false) => UniformInner::Passthrough(input),
            (true, false) => {
                let rate_converted = SampleRateConverter::new(input, target_sample_rate, config);
                UniformInner::SampleRate(rate_converted)
            }
            (false, true) => {
                let channel_converted =
                    ChannelCountConverter::new(input, from_channels, target_channels);
                UniformInner::ChannelCount(channel_converted)
            }
            (true, true) => {
                let rate_converted = SampleRateConverter::new(input, target_sample_rate, config);
                let channel_converted =
                    ChannelCountConverter::new(rate_converted, from_channels, target_channels);
                UniformInner::Both(channel_converted)
            }
        }
    }
}

impl<I> Iterator for UniformSourceIterator<I>
where
    I: Source,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let mut at_boundary = self
            .cached_span_len
            .is_some_and(|span_len| self.current_span_pos >= span_len);
        // A resampler reads ahead of its input, so neither the input's
        // format nor counting spans tells where the format of what it
        // yields changes. It knows, cheaply, so ask before every sample.
        let resamples = matches!(
            self.inner,
            Some(UniformInner::SampleRate(_) | UniformInner::Both(_))
        );

        // Only between output frames: a frame is finished in the format
        // it started in.
        if (at_boundary || resamples) && self.inner.as_ref().unwrap().at_frame_start() {
            let (new_channels, new_sample_rate) = self.inner.as_ref().unwrap().next_input_format();

            let parameters_changed = new_channels != self.current_channels
                || new_sample_rate != self.current_sample_rate;

            if parameters_changed {
                let source = self.inner.take().unwrap().into_inner();
                self.current_channels = new_channels;
                self.current_sample_rate = new_sample_rate;
                let new_inner = UniformSourceIterator::bootstrap(
                    source,
                    self.target_channels,
                    self.target_sample_rate,
                );
                self.inner = Some(new_inner);
                at_boundary = true;
            }

            if at_boundary {
                let new_span_len = self.inner.as_ref().unwrap().current_span_len();

                self.current_span_pos = 0;
                self.cached_span_len = new_span_len;
            }
        }

        if let Some(sample) = self.inner.as_mut().unwrap().next() {
            // Only increment counter when tracking spans
            if self.cached_span_len.is_some() {
                self.current_span_pos += 1;
            }
            return Some(sample);
        }

        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.inner.as_ref().unwrap().size_hint().0, None)
    }
}

impl<I> Source for UniformSourceIterator<I>
where
    I: Iterator + Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.target_channels
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.target_sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.inner.as_ref().unwrap().inner().total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        if let Some(input) = self.inner.as_mut() {
            input.inner_mut().try_seek(pos)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UniformSourceIterator;
    use crate::buffer::SamplesBuffer;
    use crate::math::nz;
    use crate::source::chain;
    use crate::{Sample, Source};

    /// A mono span, then a stereo one whose channels differ, so a sample
    /// lost or added at the switch swaps every channel after it.
    fn mono_then_stereo(mono_rate: u32, stereo_rate: u32) -> impl Source {
        chain([
            SamplesBuffer::new(nz!(1), mono_rate.try_into().unwrap(), vec![0.25; 300]),
            SamplesBuffer::new(
                nz!(2),
                stereo_rate.try_into().unwrap(),
                [0.5, 0.75].repeat(200),
            ),
        ])
    }

    fn assert_frames_aligned(out: &[Sample], stereo_from: usize) {
        assert_eq!(out.len(), stereo_from + 400, "length");
        for (i, frame) in out[..stereo_from].chunks(2).enumerate() {
            assert_eq!(frame[0], frame[1], "upmixed mono frame {i}");
        }
        for (i, frame) in out[stereo_from..].chunks(2).enumerate() {
            assert_eq!(frame, [0.5, 0.75], "stereo frame {i}");
        }
    }

    /// An upmixed frame is finished before the format changes, even
    /// once the resampler has already moved on to the next span.
    #[test]
    fn upmixed_frame_ends_before_a_resampled_format_change() {
        let out: Vec<_> =
            UniformSourceIterator::new(mono_then_stereo(24000, 48000), nz!(2), nz!(48000))
                .collect();
        assert_frames_aligned(&out, 1200);
    }

    #[test]
    fn upmixed_frame_ends_before_a_format_change() {
        let out: Vec<_> =
            UniformSourceIterator::new(mono_then_stereo(48000, 48000), nz!(2), nz!(48000))
                .collect();
        assert_frames_aligned(&out, 600);
    }

    #[test]
    fn upmixed_frame_ends_before_a_rate_change() {
        let out: Vec<_> =
            UniformSourceIterator::new(mono_then_stereo(24000, 24000), nz!(2), nz!(48000))
                .collect();
        // Both spans resampled: the stereo one doubles to 400 frames.
        // Its first frame blends in the mono span's last sample, and its
        // last one the silence after the end.
        assert_eq!(out.len(), 1200 + 800, "length");
        for (i, frame) in out[..1200].chunks(2).enumerate() {
            assert_eq!(frame[0], frame[1], "upmixed mono frame {i}");
        }
        for (i, frame) in out[1202..out.len() - 2].chunks(2).enumerate() {
            assert_eq!(frame, [0.5, 0.75], "stereo frame {i}");
        }
    }
}
