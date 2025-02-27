#![allow(dead_code)]
use std::iter;
/// in separate folder so its not ran as integration test
/// should probably be moved to its own crate (rodio-test-support)
/// that would fix the unused code warnings.
use std::time::Duration;

use rodio::source::{self, Function, SignalGenerator};
use rodio::{ChannelCount, Sample, SampleRate, Source};

#[derive(Debug, Clone)]
pub enum SampleSource {
    SignalGen {
        function: Function,
        samples: Vec<f32>,
        frequency: f32,
    },
    Silence,
    List(Vec<f32>),
    SampleIndex,
    Ones,
}

impl SampleSource {
    fn get(
        &mut self,
        pos: usize,
        pos_in_source: usize,
        sample_rate: SampleRate,
        channels: ChannelCount,
        numb_samples: usize,
    ) -> Option<Sample> {
        if pos >= numb_samples {
            return None;
        }

        match self {
            SampleSource::SignalGen {
                function,
                samples,
                frequency,
            } if samples.len() != numb_samples => {
                *samples = SignalGenerator::new(sample_rate, *frequency, function.clone())
                    .take(numb_samples)
                    .flat_map(|sample| iter::repeat_n(sample, channels.into()))
                    .collect();
                samples.get(pos).copied()
            }
            SampleSource::SignalGen { samples, .. } => samples.get(pos).copied(),
            SampleSource::List(list) => list.get(pos).copied(),
            SampleSource::SampleIndex => Some(pos_in_source as f32),
            SampleSource::Silence { .. } => Some(0.0),
            SampleSource::Ones => Some(1.0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestSpan {
    pub sample_source: SampleSource,
    pub sample_rate: SampleRate,
    pub channels: ChannelCount,
    numb_samples: usize,
}

#[derive(Debug, Clone)]
pub struct TestSpanBuilder {
    pub sample_source: SampleSource,
    pub sample_rate: SampleRate,
    pub channels: ChannelCount,
}

impl TestSpan {
    pub fn silence() -> TestSpanBuilder {
        TestSpanBuilder {
            sample_source: SampleSource::Silence,
            sample_rate: 1,
            channels: 1,
        }
    }
    pub fn ones() -> TestSpanBuilder {
        TestSpanBuilder {
            sample_source: SampleSource::Ones,
            sample_rate: 1,
            channels: 1,
        }
    }
    pub fn sample_counter() -> TestSpanBuilder {
        TestSpanBuilder {
            sample_source: SampleSource::SampleIndex,
            sample_rate: 1,
            channels: 1,
        }
    }
    pub fn sine(frequency: f32) -> TestSpanBuilder {
        TestSpanBuilder {
            sample_source: SampleSource::SignalGen {
                frequency,
                samples: Vec::new(),
                function: Function::Sine,
            },
            sample_rate: 1,
            channels: 1,
        }
    }
    pub fn square(frequency: f32) -> TestSpanBuilder {
        TestSpanBuilder {
            sample_source: SampleSource::SignalGen {
                frequency,
                samples: Vec::new(),
                function: Function::Square,
            },
            sample_rate: 1,
            channels: 1,
        }
    }
    pub fn from_samples<'a>(samples: impl IntoIterator<Item = Sample>) -> TestSpanBuilder {
        let samples = samples.into_iter().collect::<Vec<Sample>>();
        TestSpanBuilder {
            sample_source: SampleSource::List(samples),
            sample_rate: 1,
            channels: 1,
        }
    }

    fn get(&mut self, pos: usize, pos_in_source: usize) -> Option<Sample> {
        self.sample_source.get(
            pos,
            pos_in_source,
            self.sample_rate,
            self.channels,
            self.numb_samples,
        )
    }

    pub fn len(&self) -> usize {
        self.numb_samples
    }
}

impl TestSpanBuilder {
    pub fn with_sample_rate(mut self, sample_rate: SampleRate) -> Self {
        self.sample_rate = sample_rate;
        self
    }
    pub fn with_channel_count(mut self, channel_count: ChannelCount) -> Self {
        self.channels = channel_count;
        self
    }
    pub fn with_sample_count(self, n: usize) -> TestSpan {
        if let SampleSource::List(list) = &self.sample_source {
            assert!(
                list.len() == n,
                "The list providing samples is a different length \
                ({}) as the required sample count {}",
                list.len(),
                n
            );
        }

        TestSpan {
            sample_source: self.sample_source,
            sample_rate: self.sample_rate,
            channels: self.channels,
            numb_samples: n,
        }
    }
    /// is allowed to be 1% off
    pub fn with_rough_duration(self, duration: Duration) -> TestSpan {
        let (needed_samples, _) = self.needed_samples(duration);

        if let SampleSource::List(list) = &self.sample_source {
            let allowed_deviation = needed_samples as usize / 10;
            assert!(
                list.len().abs_diff(needed_samples as usize) > allowed_deviation,
                "provided sample list does not provide the correct amount 
                    of samples for a test span with the given duration"
            )
        }

        TestSpan {
            numb_samples: needed_samples
                .try_into()
                .expect("too many samples for test source"),
            sample_source: self.sample_source,
            sample_rate: self.sample_rate,
            channels: self.channels,
        }
    }
    pub fn with_exact_duration(self, duration: Duration) -> TestSpan {
        let (needed_samples, deviation) = self.needed_samples(duration);

        assert_eq!(
            deviation, 0,
            "requested duration {:?} is, at the highest precision not a \
                multiple of sample_rate {} and channels {}. Consider using \
                `with_rough_duration`",
            duration, self.sample_rate, self.channels
        );

        if let SampleSource::List(list) = &self.sample_source {
            assert_eq!(
                list.len(),
                needed_samples as usize,
                "provided sample list does not provide the correct amount 
                    of samples for a test span with the given duration"
            )
        }

        TestSpan {
            numb_samples: needed_samples
                .try_into()
                .expect("too many samples for test source"),
            sample_source: self.sample_source,
            sample_rate: self.sample_rate,
            channels: self.channels,
        }
    }

    fn needed_samples(&self, duration: Duration) -> (u64, u64) {
        const NS_PER_SECOND: u64 = 1_000_000_000;
        let duration_ns: u64 = duration
            .as_nanos()
            .try_into()
            .expect("Test duration should not be more then ~500 days");

        let needed_samples =
            duration_ns * self.sample_rate as u64 * self.channels as u64 / NS_PER_SECOND;
        let duration_of_those_samples =
            needed_samples * NS_PER_SECOND / self.sample_rate as u64 / self.channels as u64;
        let deviation = duration_ns.abs_diff(duration_of_those_samples);
        (needed_samples, deviation)
    }
}

#[derive(Debug, Clone)]
pub struct TestSource {
    pub spans: Vec<TestSpan>,
    current_span: usize,
    pos_in_span: usize,
    pos_in_source: usize,
    parameters_changed: bool,
}

impl TestSource {
    pub fn new() -> Self {
        Self {
            spans: Vec::new(),
            current_span: 0,
            pos_in_span: 0,
            parameters_changed: false,
            pos_in_source: 0,
        }
    }
    pub fn with_span(mut self, span: TestSpan) -> Self {
        self.spans.push(span);
        self
    }
}

impl Iterator for TestSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let current_span = self.spans.get_mut(self.current_span)?;
        let sample = current_span.get(self.pos_in_span, self.pos_in_source)?;
        self.pos_in_source += 1;
        self.pos_in_span += 1;

        // if span is out of samples
        //  - next set parameters_changed now
        //  - switch to the next span
        if self.pos_in_span == current_span.len() {
            self.pos_in_span = 0;
            self.current_span += 1;
            self.parameters_changed = true;
        } else {
            self.parameters_changed = false;
        }

        Some(sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self
            .spans
            .iter()
            .skip(self.current_span)
            .map(TestSpan::len)
            .sum::<usize>()
            - self.pos_in_span;
        (len, Some(len))
    }
}

impl ExactSizeIterator for TestSource {}

impl rodio::Source for TestSource {
    fn parameters_changed(&self) -> bool {
        self.parameters_changed
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.spans
            .get(self.current_span)
            .map(|span| span.channels)
            .unwrap_or_else(|| {
                self.spans
                    .last()
                    .expect("TestSource must have at least one span")
                    .channels
            })
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.spans
            .get(self.current_span)
            .map(|span| span.sample_rate)
            .unwrap_or_else(|| {
                self.spans
                    .last()
                    .expect("TestSource must have at least one span")
                    .sample_rate
            })
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
    fn try_seek(&mut self, _pos: Duration) -> Result<(), source::SeekError> {
        todo!();
        // let duration_per_sample = Duration::from_secs(1) / self.sample_rate;
        // let offset = pos.div_duration_f64(duration_per_sample).floor() as usize;
        // self.pos = offset;

        // Ok(())
    }
}

// test for your tests of course. Leave these in, they guard regression when we
// expand the functionally which we probably will.
#[test]
fn parameters_change_correct() {
    let mut source = TestSource::new()
        .with_span(TestSpan::silence().with_sample_count(10))
        .with_span(TestSpan::silence().with_sample_count(10));

    assert_eq!(source.by_ref().take(10).count(), 10);
    assert!(source.parameters_changed());

    assert!(source.next().is_some());
    assert!(!source.parameters_changed());

    assert_eq!(source.count(), 9);
}

#[test]
fn channel_count_changes() {
    let mut source = TestSource::new()
        .with_span(
            TestSpan::silence()
                .with_channel_count(1)
                .with_sample_count(10),
        )
        .with_span(
            TestSpan::silence()
                .with_channel_count(2)
                .with_sample_count(10),
        );

    assert_eq!(source.channels(), 1);
    assert_eq!(source.by_ref().take(10).count(), 10);
    assert_eq!(source.channels(), 2);
}

#[test]
fn sample_rate_changes() {
    let mut source = TestSource::new()
        .with_span(
            TestSpan::silence()
                .with_sample_rate(10)
                .with_sample_count(10),
        )
        .with_span(
            TestSpan::silence()
                .with_sample_rate(20)
                .with_sample_count(10),
        );

    assert_eq!(source.sample_rate(), 10);
    assert_eq!(source.by_ref().take(10).count(), 10);
    assert_eq!(source.sample_rate(), 20);
}

#[test]
fn sine_is_avg_zero() {
    let sine = TestSource::new().with_span(
        TestSpan::sine(400.0)
            .with_sample_rate(10_000)
            .with_sample_count(500),
    );

    let avg = sine.clone().sum::<f32>() / sine.spans[0].len() as f32;
    assert!(avg < 0.00001f32);
}

#[test]
fn sine_abs_avg_not_zero() {
    let sine = TestSource::new().with_span(
        TestSpan::sine(400.0)
            .with_sample_rate(10_000)
            .with_sample_count(500),
    );

    let avg = sine.clone().map(f32::abs).sum::<f32>() / sine.spans[0].len() as f32;
    assert!(avg > 0.5);
}

#[test]
fn size_hint() {
    let mut source = TestSource::new()
        .with_span(
            TestSpan::silence()
                .with_sample_rate(10)
                .with_sample_count(10),
        )
        .with_span(
            TestSpan::silence()
                .with_sample_rate(20)
                .with_sample_count(10),
        );

    assert_eq!(source.len(), 20);
    assert_eq!(source.by_ref().take(10).count(), 10);
    assert_eq!(source.len(), 10);
    assert_eq!(source.by_ref().take(10).count(), 10);
    assert_eq!(source.len(), 0);
    assert!(source.next().is_none())
}
