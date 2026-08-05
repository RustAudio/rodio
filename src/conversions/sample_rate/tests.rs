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
