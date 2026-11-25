use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::conversions::ChannelCountConverter;
use crate::conversions::{Poly, ResampleConfig, SampleRateConverter};
use crate::{Sample, Source};

#[derive(Clone)]
enum UniformInner<I: Source> {
    Passthrough(I),
    SampleRate(SampleRateConverter<I>),
    ChannelCount(ChannelCountConverter<I>),
    BothUpmix(ChannelCountConverter<SampleRateConverter<I>>),
    // Downmixed first, so only the channels kept are resampled.
    BothDownmix(SampleRateConverter<Downmix<I>>),
}

/// Drops channels ahead of a resampler. A resampler reads ahead across
/// spans, and stops where its input's format changes. So once the
/// input's channel count is no longer the one converted from, this
/// reports another count than the one it converts to, which stops the
/// resampler there as the raw input would have.
#[derive(Clone)]
struct Downmix<I: Source>(ChannelCountConverter<I>);

impl<I: Source> Downmix<I> {
    #[inline]
    fn new(input: I, from: ChannelCount, to: ChannelCount) -> Self {
        Self(ChannelCountConverter::new(input, from, to))
    }

    /// Whether the input is still in the channel count converted from.
    #[inline]
    fn input_unchanged(&self) -> bool {
        self.0.inner().channels() == self.0.channels_from()
    }
}

impl<I: Source> Iterator for Downmix<I> {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Sample> {
        self.0.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<I: Source> Source for Downmix<I> {
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        if self.input_unchanged() {
            self.0.current_span_len()
        } else {
            // Converted, a span shorter than a frame of the old format
            // would count as `Some(0)`: the end of the stream.
            self.0.inner().current_span_len()
        }
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        let input = self.0.inner().channels();
        if self.input_unchanged() {
            self.0.channels()
        } else if input != self.0.channels() {
            input
        } else {
            // Any count but the converted one stops the resampler.
            self.0.channels_from()
        }
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.0.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.0.total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.0.try_seek(pos)
    }
}

impl<I: Source> Iterator for UniformInner<I> {
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            UniformInner::Passthrough(take) => take.next(),
            UniformInner::SampleRate(converter) => converter.next(),
            UniformInner::ChannelCount(converter) => converter.next(),
            UniformInner::BothUpmix(converter) => converter.next(),
            UniformInner::BothDownmix(converter) => converter.next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            UniformInner::Passthrough(take) => take.size_hint(),
            UniformInner::SampleRate(converter) => converter.size_hint(),
            UniformInner::ChannelCount(converter) => converter.size_hint(),
            UniformInner::BothUpmix(converter) => converter.size_hint(),
            UniformInner::BothDownmix(converter) => converter.size_hint(),
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
            UniformInner::BothUpmix(converter) => converter.into_inner().into_inner(),
            UniformInner::BothDownmix(converter) => converter.into_inner().0.into_inner(),
        }
    }

    #[inline]
    fn inner(&self) -> &I {
        match self {
            UniformInner::Passthrough(source) => source,
            UniformInner::SampleRate(converter) => converter.inner(),
            UniformInner::ChannelCount(converter) => converter.inner(),
            UniformInner::BothUpmix(converter) => converter.inner().inner(),
            UniformInner::BothDownmix(converter) => converter.inner().0.inner(),
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
            UniformInner::BothUpmix(converter) => converter.inner().next_input_format(),
            UniformInner::BothDownmix(converter) => {
                let downmix = &converter.inner().0;
                if converter.holds_samples() {
                    (downmix.channels_from(), converter.next_input_format().1)
                } else {
                    let input = downmix.inner();
                    (input.channels(), input.sample_rate())
                }
            }
        }
    }

    /// Whether the next sample starts an output frame. Mid-frame, the
    /// channel converter still owes samples of input it has read, so
    /// rebuilding there would drop them and shift every later channel.
    #[inline]
    fn at_frame_start(&self) -> bool {
        match self {
            // A resampler yields whole frames from its buffer.
            UniformInner::Passthrough(_)
            | UniformInner::SampleRate(_)
            | UniformInner::BothDownmix(_) => true,
            UniformInner::ChannelCount(converter) => converter.at_frame_start(),
            UniformInner::BothUpmix(converter) => converter.at_frame_start(),
        }
    }

    #[inline]
    fn inner_mut(&mut self) -> &mut I {
        match self {
            UniformInner::Passthrough(source) => source,
            UniformInner::SampleRate(converter) => converter.inner_mut(),
            UniformInner::ChannelCount(converter) => converter.inner_mut(),
            UniformInner::BothUpmix(converter) => converter.inner_mut().inner_mut(),
            UniformInner::BothDownmix(converter) => converter.inner_mut().0.inner_mut(),
        }
    }
}

impl<I: Source> Source for UniformInner<I> {
    /// How many samples to count before checking the format again. A
    /// resampler reads ahead of its input, so neither the input's spans
    /// nor its own tell where the format of what it yields changes. What
    /// it has buffered is all in one format, though, so that's counted.
    /// Only called at the start of an output frame.
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        match self {
            UniformInner::Passthrough(source) => source.current_span_len(),
            UniformInner::SampleRate(converter) => Some(converter.samples_in_format()),
            UniformInner::ChannelCount(converter) => converter.current_span_len(),
            UniformInner::BothUpmix(converter) => {
                Some(converter.converted_len(converter.inner().samples_in_format()))
            }
            UniformInner::BothDownmix(converter) => Some(converter.samples_in_format()),
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
            (true, true) if target_channels > from_channels => {
                let rate_converted = SampleRateConverter::new(input, target_sample_rate, config);
                let channel_converted =
                    ChannelCountConverter::new(rate_converted, from_channels, target_channels);
                UniformInner::BothUpmix(channel_converted)
            }
            (true, true) => {
                let channel_converted = Downmix::new(input, from_channels, target_channels);
                let rate_converted =
                    SampleRateConverter::new(channel_converted, target_sample_rate, config);
                UniformInner::BothDownmix(rate_converted)
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
        if let Some(span_len) = self.cached_span_len {
            // Only between output frames: a frame is finished in the
            // format it started in.
            if self.current_span_pos >= span_len && self.inner.as_ref().unwrap().at_frame_start() {
                let (new_channels, new_sample_rate) =
                    self.inner.as_ref().unwrap().next_input_format();

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
                }

                self.current_span_pos = 0;
                self.cached_span_len = self.inner.as_ref().unwrap().current_span_len();
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

    /// Downmixed ahead of the resampler, a span still ends on its last
    /// frame when the next one, in another format, is shorter than a
    /// frame of it.
    #[test]
    fn downmixed_span_ends_on_its_input_before_a_short_span() {
        let source = chain([
            SamplesBuffer::new(nz!(2), nz!(24000), [0.5, -0.25].repeat(300)),
            SamplesBuffer::new(nz!(1), nz!(48000), vec![0.75]),
        ]);
        let out: Vec<_> = UniformSourceIterator::new(source, nz!(1), nz!(48000)).collect();
        assert_eq!(out.len(), 600 + 1, "length");
        for (i, sample) in out[..600].iter().enumerate() {
            assert!((sample - 0.5).abs() < 1e-6, "sample {i} is {sample}");
        }
        assert_eq!(out[600], 0.75);
    }

    /// A resampler reading downmixed input stops where the input's
    /// channel count changes, even to the one downmixed to.
    #[test]
    fn downmixed_input_changing_to_the_target_channel_count() {
        let source = chain([
            SamplesBuffer::new(nz!(2), nz!(24000), [0.5, -0.25].repeat(300)),
            SamplesBuffer::new(nz!(1), nz!(24000), vec![0.75; 300]),
        ]);
        let out: Vec<_> = UniformSourceIterator::new(source, nz!(1), nz!(48000)).collect();
        assert_eq!(out.len(), 600 + 600, "length");
        for (i, sample) in out[..600].iter().enumerate() {
            assert!((sample - 0.5).abs() < 1e-6, "sample {i} is {sample}");
        }
        // Past the resampler's first frame, and before the end's silence.
        for (i, sample) in out[602..1198].iter().enumerate() {
            assert!(
                (sample - 0.75).abs() < 1e-6,
                "sample {} is {sample}",
                602 + i
            );
        }
    }

    /// A resampler rebuilt at a rate change, as `Player::set_speed` makes
    /// one, starts on the new span's first frame rather than dipping
    /// towards silence.
    #[test]
    fn rate_change_starts_on_the_input() {
        let source = chain([
            SamplesBuffer::new(nz!(2), nz!(110250), [0.5, -0.5].repeat(2205)),
            SamplesBuffer::new(nz!(2), nz!(44100), [0.8, -0.6].repeat(4410)),
        ]);
        let out: Vec<Sample> = UniformSourceIterator::new(source, nz!(2), nz!(48000)).collect();
        // The end of the stream falls to silence: leave it out.
        let frames = out.chunks(2).take(out.len() / 2 - 4);
        let first_after = 960;
        for (i, frame) in frames.enumerate() {
            let expected = if i < first_after {
                [0.5, -0.5]
            } else {
                [0.8, -0.6]
            };
            assert_eq!(frame, expected, "frame {i}");
        }
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
