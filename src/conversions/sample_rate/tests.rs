use super::InFrameCount;
use super::*;
use crate::source::{from_iter, SineWave};
use crate::{nz, Source};
use dasp_sample::ToSample;
use quickcheck::{quickcheck, Arbitrary, Gen, TestResult};
use std::num::NonZero;

#[derive(Debug, Clone, Copy)]
struct TestSampleRate(SampleRate);

impl Arbitrary for TestSampleRate {
    fn arbitrary(g: &mut Gen) -> Self {
        // Generate realistic sample rates: 8 kHz to 384 kHz
        let rate = u32::arbitrary(g) % 376_001 + 8_000;
        TestSampleRate(SampleRate::new(rate).unwrap())
    }
}

impl std::ops::Deref for TestSampleRate {
    type Target = SampleRate;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Copy)]
struct TestChannelCount(ChannelCount);

impl Arbitrary for TestChannelCount {
    fn arbitrary(g: &mut Gen) -> Self {
        // Generate realistic channel counts: 1 to 8
        let channels = (u16::arbitrary(g) % 7) + 1;
        TestChannelCount(ChannelCount::new(channels).unwrap())
    }
}

impl std::ops::Deref for TestChannelCount {
    type Target = ChannelCount;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

struct TestSpan {
    samples: Vec<Sample>,
    rate: SampleRate,
    channels: ChannelCount,
}

/// Multi-span test source.
///
/// Build with [`TestSource::new`] and extend with [`.chain()`](TestSource::chain).
struct TestSource {
    spans: Vec<TestSpan>,
    span: usize,
    offset: usize,
}

impl TestSource {
    fn new(samples: Vec<Sample>, rate: SampleRate, channels: ChannelCount) -> Self {
        Self {
            spans: vec![TestSpan {
                samples,
                rate,
                channels,
            }],
            span: 0,
            offset: 0,
        }
    }

    fn chain(mut self, samples: Vec<Sample>, rate: SampleRate, channels: ChannelCount) -> Self {
        self.spans.push(TestSpan {
            samples,
            rate,
            channels,
        });
        self
    }

    /// Returns the active span for metadata queries.
    fn current_span(&self) -> &TestSpan {
        self.spans
            .get(self.span)
            .unwrap_or_else(|| self.spans.last().unwrap())
    }
}

impl Iterator for TestSource {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.span >= self.spans.len() {
            return None;
        }
        let s = self.spans[self.span].samples[self.offset];
        self.offset += 1;
        if self.offset >= self.spans[self.span].samples.len() {
            self.span += 1;
            self.offset = 0;
        }
        Some(s)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.spans.iter().skip(self.span).map(|s| s.samples.len());
        let left = left.sum::<usize>() - self.offset;
        (left, Some(left))
    }
}

impl Source for TestSource {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.spans.get(self.span).map_or(0, |s| s.samples.len()))
    }

    fn sample_rate(&self) -> SampleRate {
        self.current_span().rate
    }

    fn channels(&self) -> ChannelCount {
        self.current_span().channels
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(
            self.spans
                .iter()
                .map(|s| {
                    let frames = s.samples.len() / s.channels.get() as usize;
                    Duration::from_secs_f64(frames as f64 / s.rate.get() as f64)
                })
                .sum(),
        )
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        let position = self.total_duration().map_or(position, |d| position.min(d));
        let mut remaining = position.as_secs_f64();
        for (i, span) in self.spans.iter().enumerate() {
            let frames = span.samples.len() / span.channels.get() as usize;
            let span_dur = frames as f64 / span.rate.get() as f64;
            let is_last = i + 1 == self.spans.len();
            if remaining < span_dur || is_last {
                let frame_offset = (remaining * span.rate.get() as f64) as usize;
                let sample_offset =
                    (frame_offset * span.channels.get() as usize).min(span.samples.len());
                self.span = i;
                self.offset = sample_offset;
                if self.offset >= span.samples.len() {
                    self.span += 1;
                    self.offset = 0;
                }
                return Ok(());
            }
            remaining -= span_dur;
        }
        unreachable!("TestSource always has at least one span")
    }
}

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
    fn empty(from: TestSampleRate, to: TestSampleRate, channels: TestChannelCount) -> bool {
        let input = vec![];
        let config = ResampleConfig::default();
        let source = from_iter(input.clone().into_iter(), *channels, *from);
        let output = SampleRateConverter::new(source, *to, config).collect::<Vec<_>>();
        input == output
    }

    /// Check that resampling to the same rate does not change the signal.
    fn identity(from: TestSampleRate, channels: TestChannelCount, input: Vec<i16>) -> bool {
        let input = convert_to_frames(input, *channels);
        let config = ResampleConfig::default();
        let source = from_iter(input.clone().into_iter(), *channels, *from);
        let output = SampleRateConverter::new(source, *from, config).collect::<Vec<_>>();
        input == output
    }

    /// Check that resampling does not change the audio duration, except by a negligible
    /// amount (± 1ms). Reproduces #316.
    fn preserve_durations(d: Duration, freq: f32, to: TestSampleRate) -> TestResult {
        use crate::source::{SineWave, Source};
        if !freq.is_normal() || freq <= 0.0 || d > Duration::from_secs(1) {
            return TestResult::discard();
        }

        let source = SineWave::new(freq).take_duration(d);
        let from = source.sample_rate();

        let config = ResampleConfig::poly().degree(Poly::Linear).build();
        let resampled = SampleRateConverter::new(source, *to, config);
        let duration = Duration::from_secs_f32(resampled.count() as f32 / to.get() as f32);

        let delta = duration.abs_diff(d);
        TestResult::from_bool(delta < Duration::from_millis(1))
    }
}

/// Helper to create interleaved multi-channel test data using SineWave sources.
fn create_test_input(frames: InFrameCount, channels: ChannelCount) -> Vec<Sample> {
    let frequencies = [440.0, 1000.0];
    let mut input = Vec::new();

    // Create a SineWave for each channel
    let mut waves: Vec<_> = (0..channels.get())
        .map(|ch| SineWave::new(frequencies[ch as usize % frequencies.len()]))
        .collect();

    // Interleave samples from each channel
    for _ in 0..frames.raw() {
        for wave in waves.iter_mut() {
            input.push(wave.next().unwrap());
        }
    }
    input
}

/// Test various ratio types: integer, fractional, and reciprocal.
#[test]
fn test_sample_rate_conversions() {
    let test_cases = [
        // (from_rate, to_rate, channels, description)
        (1000, 7000, 1, "integer upsample 7x"),
        (2000, 3000, 2, "fractional upsample 1.5x"),
        (12000, 2400, 1, "integer downsample 1/5x"),
        (48000, 44100, 2, "fractional downsample (DVD to CD)"),
        (8000, 48001, 1, "async sinc"),
    ];

    let configs: &[(&str, ResampleConfig)] = &[
        ("poly", ResampleConfig::poly().build()),
        ("sinc", ResampleConfig::sinc().build()),
    ];

    for (config_name, config) in configs {
        for (from_rate, to_rate, channels, desc) in test_cases {
            let from = SampleRate::new(from_rate).unwrap();
            let to = SampleRate::new(to_rate).unwrap();
            let ch = ChannelCount::new(channels).unwrap();

            let input_frames = InFrameCount(100);
            let input = create_test_input(input_frames, ch);
            let input_samples = input.len();

            let source = from_iter(input.into_iter(), ch, from);
            let resampler = SampleRateConverter::new(source, to, config.clone());

            let size_hint_lower = resampler.size_hint().0;
            let output_count = resampler.count();

            assert_eq!(
                output_count, size_hint_lower,
                "[{config_name}] {desc}: size_hint {size_hint_lower} should equal actual output {output_count}",
            );

            let ratio = to.get() as f64 / from.get() as f64;
            let expected_samples = (input_samples as f64 * ratio).ceil() as usize;

            assert_eq!(
                output_count.abs_diff(expected_samples),
                0,
                "[{config_name}] {desc}: expected {expected_samples} samples, got {output_count}",
            );
        }
    }
}

#[test]
fn test_current_span_len_excludes_delay() {
    let channels = ChannelCount::new(1).unwrap();
    let from = SampleRate::new(44100).unwrap();
    let to = SampleRate::new(48000).unwrap();

    let input = create_test_input(InFrameCount(2048), channels);
    let source = from_iter(input.into_iter(), channels, from);
    // sinc_len=16 gives a non-zero output delay without being slow in debug builds
    let config = ResampleConfig::sinc()
        .sinc_len(NonZero::new(16).unwrap())
        .build();
    let mut resampler = SampleRateConverter::new(source, to, config);

    let _ = resampler.next().expect("should have samples");
    let reported = resampler
        .current_span_len()
        .expect("should report span len");
    assert!(
        reported > channels.get() as usize,
        "after yielding a sample the span should not be just one frame"
    );

    let mut count = 1;
    while resampler.current_span_len() == Some(reported) {
        assert!(
            resampler.next().is_some(),
            "source exhausted before first chunk drained"
        );
        count += 1;
    }

    assert_eq!(
        count, reported,
        "current_span_len() = {reported} but first chunk emitted {count} samples"
    );
}

#[test]
fn test_span_boundary_same_format() {
    let span_frames = 100usize;
    let channels = ChannelCount::new(1).unwrap();
    let rate = SampleRate::new(44100).unwrap();
    let target = SampleRate::new(48000).unwrap();

    let source = TestSource::new(vec![0.1; span_frames], rate, channels).chain(
        vec![0.9; span_frames],
        rate,
        channels,
    );

    let output: Vec<Sample> =
        SampleRateConverter::new(source, target, ResampleConfig::poly().build()).collect();

    let ratio = target.get() as f64 / rate.get() as f64;
    let expected = ((2 * span_frames) as f64 * ratio).ceil() as usize;

    assert_eq!(
        output.len(),
        expected,
        "expected {expected} samples from both spans, got {} \
             (second span likely not processed)",
        output.len()
    );
}

/// Split `input` into spans of `span_samples`, all with the same format.
fn chunked_into_spans(
    input: &[Sample],
    span_samples: usize,
    rate: SampleRate,
    channels: ChannelCount,
) -> TestSource {
    let mut spans = input.chunks(span_samples);
    let first = spans.next().expect("input is not empty").to_vec();
    spans.fold(TestSource::new(first, rate, channels), |source, span| {
        source.chain(span.to_vec(), rate, channels)
    })
}

/// Same samples, same format, different span lengths must resample to
/// the same output: a span boundary without a format change is not a
/// discontinuity. Reproduces #907, where Vorbis-sized spans made every
/// chunk look like end-of-stream and get zero-padded.
#[test]
fn output_is_independent_of_span_chunking() {
    let channels = ChannelCount::new(2).unwrap();
    let rate = SampleRate::new(44100).unwrap();
    let target = SampleRate::new(48000).unwrap();
    let input = create_test_input(InFrameCount(4410), channels);
    let config = || ResampleConfig::poly().build();

    let whole: Vec<Sample> = SampleRateConverter::new(
        TestSource::new(input.clone(), rate, channels),
        target,
        config(),
    )
    .collect();

    // 128 frames is a typical Vorbis short block.
    let source = chunked_into_spans(&input, 128 * channels.get() as usize, rate, channels);
    let chunked: Vec<Sample> = SampleRateConverter::new(source, target, config()).collect();

    assert_eq!(whole.len(), chunked.len(), "output length changed");
    let worst = whole
        .iter()
        .zip(&chunked)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, Sample::max);
    assert!(worst < 1e-4, "output differs by up to {worst}");
}

/// A span that changes the sample rate must be resampled at its own
/// rate. It used to be resampled at the previous span's rate, so a
/// span that halved the rate came out half as long as it should.
#[test]
fn span_boundary_may_change_sample_rate() {
    let channels = ChannelCount::new(1).unwrap();
    let target = SampleRate::new(48000).unwrap();
    let (fast, slow) = (
        SampleRate::new(44100).unwrap(),
        SampleRate::new(22050).unwrap(),
    );
    let frames = InFrameCount(441);

    let source = TestSource::new(create_test_input(frames, channels), fast, channels).chain(
        create_test_input(frames, channels),
        slow,
        channels,
    );
    let output: Vec<Sample> =
        SampleRateConverter::new(source, target, ResampleConfig::poly().build()).collect();

    let expected = resampled_len(frames, fast, target) + resampled_len(frames, slow, target);
    assert_close(output.len(), expected, "sample rate change");
}

/// The same for a span that changes the channel count.
#[test]
fn span_boundary_may_change_channel_count() {
    let (stereo, mono) = (ChannelCount::new(2).unwrap(), ChannelCount::new(1).unwrap());
    let rate = SampleRate::new(44100).unwrap();
    let target = SampleRate::new(48000).unwrap();
    let frames = InFrameCount(441);

    let source = TestSource::new(create_test_input(frames, stereo), rate, stereo).chain(
        create_test_input(frames, mono),
        rate,
        mono,
    );
    let output: Vec<Sample> =
        SampleRateConverter::new(source, target, ResampleConfig::poly().build()).collect();

    let expected = resampled_len(frames, rate, target) * 2 + resampled_len(frames, rate, target);
    assert_close(output.len(), expected, "channel count change");
}

/// Samples one span of `frames` becomes at `target`, one channel.
fn resampled_len(frames: InFrameCount, from: SampleRate, target: SampleRate) -> usize {
    (frames.raw() as f64 * target.get() as f64 / from.get() as f64).round() as usize
}

/// Resampler delay and rounding move the total by a few samples, so
/// compare with room for that rather than exactly.
fn assert_close(got: usize, expected: usize, what: &str) {
    let slack = 16;
    assert!(
        got.abs_diff(expected) <= slack,
        "{what}: got {got} samples, expected {expected} ± {slack}",
    );
}

/// Passthrough must keep yielding when the input reports no span length.
#[test]
fn passthrough_none_span_len_yields_samples() {
    let rate = crate::DEFAULT_SAMPLE_RATE;
    let mut out =
        SampleRateConverter::new(SineWave::new(440.0), rate, ResampleConfig::poly().build());
    let n = (&mut out).take(32).count();
    assert_eq!(
        n, 32,
        "Passthrough with current_span_len() == None stopped early (got {n})"
    );
}

/// Spans of `(frames, rate, channels)`, one after the other.
fn spans_of(spans: &[(usize, u32, u16)]) -> TestSource {
    let span = |&(frames, rate, channels): &(usize, u32, u16)| {
        let channels = ChannelCount::new(channels).unwrap();
        let input = create_test_input(InFrameCount(frames), channels);
        (input, SampleRate::new(rate).unwrap(), channels)
    };
    let mut spans = spans.iter().map(span);
    let (input, rate, channels) = spans.next().expect("at least one span");
    spans.fold(TestSource::new(input, rate, channels), |source, span| {
        source.chain(span.0, span.1, span.2)
    })
}

/// A resampler reads ahead of the samples it yields, so what it reports
/// must describe the next sample, not where its input has got to.
#[test]
fn channels_follow_the_samples_still_buffered() {
    let target = SampleRate::new(48000).unwrap();
    let mut resampler = SampleRateConverter::new(
        spans_of(&[(100, 24000, 1), (100, 24000, 2)]),
        target,
        ResampleConfig::poly().build(),
    );

    let mut mono = 0;
    while resampler.channels().get() == 1 {
        resampler.next().expect("the stereo span is still to come");
        mono += 1;
    }
    assert_close(mono, 200, "mono samples before channels() turned stereo");
    let stereo = resampler.count();
    assert_close(stereo, 400, "stereo samples after");
}

#[test]
fn is_exhausted_only_once_nothing_is_left() {
    let target = SampleRate::new(44100).unwrap();
    // A sinc filter still holds samples after its input runs out.
    let config = ResampleConfig::sinc()
        .sinc_len(NonZero::new(16).unwrap())
        .build();
    let mut resampler = SampleRateConverter::new(
        spans_of(&[(429, 22050, 2), (262, 12000, 2)]),
        target,
        config,
    );

    let mut emitted = 0;
    loop {
        let exhausted = resampler.is_exhausted();
        let Some(_) = resampler.next() else {
            break;
        };
        assert!(!exhausted, "is_exhausted() before sample {emitted}");
        emitted += 1;
    }
    assert!(resampler.is_exhausted());
}

/// Later spans may be at another rate, so the current ratio can't bound
/// them: past the span being read, only the lower bound may count on it.
#[test]
fn size_hint_holds_across_rate_changes() {
    let target = SampleRate::new(48000).unwrap();
    for spans in [
        &[(64, 48000, 1), (128, 24000, 1)][..],
        &[(64, 24000, 1), (256, 96000, 1)],
        &[(128, 24000, 1), (128, 12000, 1)],
        &[(128, 24000, 1), (256, 48000, 1)],
        &[(441, 44100, 2), (441, 44100, 2)],
    ] {
        let mut resampler =
            SampleRateConverter::new(spans_of(spans), target, ResampleConfig::poly().build());
        let mut hints = vec![resampler.size_hint()];
        while resampler.next().is_some() {
            hints.push(resampler.size_hint());
        }

        let total = hints.len() - 1;
        for (emitted, (lower, upper)) in hints.into_iter().enumerate() {
            let left = total - emitted;
            assert!(
                lower <= left,
                "{spans:?}: lower bound {lower} with {left} left"
            );
            assert!(
                upper.is_none_or(|upper| upper >= left),
                "{spans:?}: upper bound {upper:?} with {left} left"
            );
        }
    }
}

/// A finished buffer in a `chain` isn't the end of the stream.
#[test]
fn chain_is_resampled_to_its_end() {
    use crate::buffer::SamplesBuffer;
    use crate::source::chain;

    let (mono, rate) = (
        ChannelCount::new(1).unwrap(),
        SampleRate::new(24000).unwrap(),
    );
    let source = chain([
        SamplesBuffer::new(mono, rate, vec![0.5; 600]),
        SamplesBuffer::new(mono, rate, vec![0.5; 64]),
    ]);
    let target = SampleRate::new(48000).unwrap();
    let output = SampleRateConverter::new(source, target, ResampleConfig::poly().build()).count();
    assert_close(output, 1328, "chain of two buffers");
}

/// The last frames before a format change are interpolated like any
/// others: not towards silence, nor with silence counted in after them.
#[test]
fn frames_before_a_format_change_follow_the_input() {
    let mono = ChannelCount::new(1).unwrap();
    let (rate, next_rate) = (
        SampleRate::new(44100).unwrap(),
        SampleRate::new(110250).unwrap(),
    );
    let frames = 20_000;
    // Many chunks, so rounding each one's output would add up.
    let ramp: Vec<Sample> = (0..frames)
        .map(|i| i as Sample / frames as Sample)
        .collect();
    let source = TestSource::new(ramp, rate, mono).chain(vec![1.0; 4410], next_rate, mono);
    let target = SampleRate::new(48000).unwrap();
    let config = ResampleConfig::poly().degree(Poly::Linear).build();
    let output: Vec<Sample> = SampleRateConverter::new(source, target, config).collect();

    let before_change = resampled_len(InFrameCount(frames), rate, target);
    let tail = &output[..before_change];
    for (i, pair) in tail.windows(2).enumerate() {
        assert!(
            pair[1] >= pair[0],
            "frame {} falls from {} to {}",
            i + 1,
            pair[0],
            pair[1]
        );
    }
    let step = 1.0 / frames as Sample;
    let last = tail.last().copied().unwrap_or_default();
    assert!(
        last >= 1.0 - 3.0 * step,
        "the last frame before the change is {last}, not the input's end"
    );
}

/// The resamplers to check a start with, each with how close to the
/// input its output has to stay: a sinc filter's gain isn't exactly one.
/// With `rubato-fft`, the sinc one is the FFT resampler at fixed ratios.
fn start_configs() -> [(&'static str, ResampleConfig, Sample); 3] {
    [
        (
            "linear",
            ResampleConfig::poly().degree(Poly::Linear).build(),
            1e-6,
        ),
        (
            "cubic",
            ResampleConfig::poly().degree(Poly::Cubic).build(),
            1e-6,
        ),
        ("balanced sinc", ResampleConfig::balanced(), 1e-3),
    ]
}

/// The first frames a resampler yields are at the input's start, so they
/// follow its first frame, not the silence rubato's history starts with.
#[test]
fn first_frames_follow_the_input() {
    let stereo = ChannelCount::new(2).unwrap();
    let target = SampleRate::new(48000).unwrap();
    for (name, config, tolerance) in start_configs() {
        for rate in [44100, 32000, 22050, 110250, 96000] {
            let rate = SampleRate::new(rate).unwrap();
            let input = [0.8, -0.6].repeat(2000);
            let source = TestSource::new(input, rate, stereo);
            let output: Vec<Sample> =
                SampleRateConverter::new(source, target, config.clone()).collect();
            for (i, frame) in output.chunks(2).take(64).enumerate() {
                assert!(
                    (frame[0] - 0.8).abs() <= tolerance && (frame[1] + 0.6).abs() <= tolerance,
                    "{name}, {rate} Hz: frame {i} is {frame:?}, not [0.8, -0.6]"
                );
            }
        }
    }
}

/// A seek starts the resampler over in the middle of the input, which
/// mustn't dip towards silence either.
#[test]
fn first_frames_after_a_seek_follow_the_input() {
    let mono = ChannelCount::new(1).unwrap();
    let rate = SampleRate::new(44100).unwrap();
    let target = SampleRate::new(48000).unwrap();
    for (name, config, tolerance) in start_configs() {
        let source = TestSource::new(vec![0.8; 44100], rate, mono);
        let mut resampler = SampleRateConverter::new(source, target, config);
        resampler.try_seek(Duration::from_millis(500)).unwrap();
        for (i, sample) in resampler.take(64).enumerate() {
            assert!(
                (sample - 0.8).abs() <= tolerance,
                "{name}: sample {i} after the seek is {sample}, not 0.8"
            );
        }
    }
}

/// With small chunks, the lead-in takes several of them, while the
/// first frame waits: it still counts as left, and comes out in place.
#[test]
fn lead_in_longer_than_a_chunk() {
    let mono = ChannelCount::new(1).unwrap();
    let (rate, target) = (
        SampleRate::new(8000).unwrap(),
        SampleRate::new(48000).unwrap(),
    );
    let config = ResampleConfig::sinc()
        .chunk_size(NonZero::new(32).unwrap())
        .build();
    let source = TestSource::new(vec![0.8; 800], rate, mono);
    let mut resampler = SampleRateConverter::new(source, target, config);

    let mut hints = vec![resampler.size_hint()];
    let mut output = Vec::new();
    loop {
        let exhausted = resampler.is_exhausted();
        let Some(sample) = resampler.next() else {
            break;
        };
        assert!(!exhausted, "is_exhausted() before sample {}", output.len());
        output.push(sample);
        hints.push(resampler.size_hint());
    }

    assert_eq!(output.len(), resampled_len(InFrameCount(800), rate, target));
    for (i, sample) in output.iter().take(256).enumerate() {
        assert!(
            (sample - 0.8).abs() <= 1e-3,
            "sample {i} is {sample}, not 0.8"
        );
    }
    for (emitted, (lower, upper)) in hints.into_iter().enumerate() {
        let left = output.len() - emitted;
        assert!(lower <= left, "lower bound {lower} with {left} left");
        assert!(
            upper.is_none_or(|upper| upper >= left),
            "upper bound {upper:?} with {left} left"
        );
    }
}

/// The copies of the first frame can fill a chunk exactly, which leaves
/// the frame itself for the next one: it still comes out first, and the
/// rest in place. A ramp through a linear resampler comes out the same at
/// every chunk size around the ones the lead-in fills exactly, from the
/// start and after a seek, as with one chunk for the whole input.
#[test]
fn first_frame_after_a_lead_in_that_fills_a_chunk() {
    let mono = ChannelCount::new(1).unwrap();
    let ramp = |frame: usize| 0.2 + frame as Sample * 1e-4;
    let input: Vec<Sample> = (0..2000).map(ramp).collect();
    // 1/64 s, a whole number of frames at 48 kHz, rounded down at 44.1.
    let seek = Duration::from_secs_f64(1.0 / 64.0);
    let resample = |rate, target, chunk, seeked| {
        let source = TestSource::new(input.clone(), rate, mono);
        let config = ResampleConfig::poly()
            .degree(Poly::Linear)
            .chunk_size(NonZero::new(chunk).unwrap())
            .build();
        let mut resampler = SampleRateConverter::new(source, target, config);
        if seeked {
            resampler.try_seek(seek).unwrap();
        }
        resampler.collect::<Vec<Sample>>()
    };
    let cases = [
        // The lead-in fills the chunk of 158 exactly.
        (44100, 48000, 140..=170),
        // And those of 144 and 145.
        (48000, 88200, 130..=160),
        // And the default chunk.
        (48000, 131456, 1024..=1024),
    ];
    for (from, to, chunks) in cases {
        let (rate, target) = (SampleRate::new(from).unwrap(), SampleRate::new(to).unwrap());
        for seeked in [false, true] {
            let first = ramp(if seeked { from as usize / 64 } else { 0 });
            let whole = resample(rate, target, 8192, seeked);
            assert!((whole[0] - first).abs() <= 1e-5, "{from} → {to} Hz");
            for chunk in chunks.clone() {
                let output = resample(rate, target, chunk, seeked);
                let what = format!("{from} → {to} Hz, chunks of {chunk}, seeked: {seeked}");
                assert_eq!(output.len(), whole.len(), "{what}");
                for (i, (&got, &expected)) in output.iter().zip(&whole).enumerate() {
                    assert!(
                        (got - expected).abs() <= 1e-5,
                        "{what}: frame {i} is {got}, not {expected}"
                    );
                }
            }
        }
    }
}

/// A single frame is still resampled, alone or before a format change.
#[test]
fn a_single_frame_is_resampled() {
    let target = SampleRate::new(48000).unwrap();
    for (name, config, _) in start_configs() {
        let alone = spans_of(&[(1, 24000, 2)]);
        let alone = SampleRateConverter::new(alone, target, config.clone()).count();
        assert_eq!(alone, 4, "{name}: one stereo frame at half the rate");

        let before_a_change = spans_of(&[(1, 24000, 2), (100, 12000, 2)]);
        let output: Vec<Sample> =
            SampleRateConverter::new(before_a_change, target, config.clone()).collect();
        assert_close(output.len(), 4 + 800, &format!("{name}: then a span"));
    }
}
