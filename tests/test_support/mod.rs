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
        numb_samples: usize,
    },
    Silence {
        numb_samples: usize,
    },
    List(Vec<f32>),
}

impl SampleSource {
    fn get(
        &mut self,
        pos: usize,
        sample_rate: SampleRate,
        channels: ChannelCount,
    ) -> Option<Sample> {
        match self {
            SampleSource::SignalGen {
                function,
                samples,
                frequency,
                numb_samples,
            } if samples.len() != *numb_samples => {
                *samples = SignalGenerator::new(sample_rate, *frequency, function.clone())
                    .take(*numb_samples)
                    .flat_map(|sample| iter::repeat_n(sample, channels.into()))
                    .collect();
                samples.get(pos).copied()
            }
            SampleSource::SignalGen { samples, .. } => samples.get(pos).copied(),
            SampleSource::Silence { numb_samples } if pos < *numb_samples => Some(0.0),
            SampleSource::Silence { .. } => None,
            SampleSource::List(list) => list.get(pos).copied(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestSpan {
    pub sample_source: SampleSource,
    pub sample_rate: SampleRate,
    pub channels: ChannelCount,
}

impl TestSpan {
    pub fn silence(numb_samples: usize) -> Self {
        Self {
            sample_source: SampleSource::Silence { numb_samples },
            sample_rate: 1,
            channels: 1,
        }
    }
    pub fn sine(frequency: f32, numb_samples: usize) -> Self {
        Self {
            sample_source: SampleSource::SignalGen {
                frequency,
                numb_samples,
                samples: Vec::new(),
                function: Function::Sine,
            },
            sample_rate: 1,
            channels: 1,
        }
    }
    pub fn square(frequency: f32, numb_samples: usize) -> Self {
        Self {
            sample_source: SampleSource::SignalGen {
                frequency,
                numb_samples,
                samples: Vec::new(),
                function: Function::Square,
            },
            sample_rate: 1,
            channels: 1,
        }
    }
    pub fn from_samples<'a>(samples: impl IntoIterator<Item = &'a f32>) -> Self {
        let samples = samples.into_iter().copied().collect::<Vec<f32>>();
        Self {
            sample_source: SampleSource::List(samples),
            sample_rate: 1,
            channels: 1,
        }
    }
    pub fn with_sample_rate(mut self, sample_rate: SampleRate) -> Self {
        self.sample_rate = sample_rate;
        self
    }
    pub fn with_channel_count(mut self, channel_count: ChannelCount) -> Self {
        self.channels = channel_count;
        self
    }
    fn get(&mut self, pos: usize) -> Option<Sample> {
        self.sample_source.get(pos, self.sample_rate, self.channels)
    }
    pub fn len(&self) -> usize {
        match &self.sample_source {
            SampleSource::SignalGen { numb_samples, .. } => *numb_samples,
            SampleSource::Silence { numb_samples } => *numb_samples,
            SampleSource::List(list) => list.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestSource {
    pub spans: Vec<TestSpan>,
    current_span: usize,
    pos_in_span: usize,
    total_duration: Option<Duration>,
    parameters_changed: bool,
}

impl TestSource {
    pub fn new() -> Self {
        Self {
            spans: Vec::new(),
            current_span: 0,
            pos_in_span: 0,
            total_duration: None,
            parameters_changed: false,
        }
    }
    pub fn with_span(mut self, span: TestSpan) -> Self {
        self.spans.push(span);
        self
    }
    pub fn with_total_duration(mut self, duration: Duration) -> Self {
        self.total_duration = Some(duration);
        self
    }
}

impl Iterator for TestSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let current_span = self.spans.get_mut(self.current_span)?;
        let sample = current_span.get(self.pos_in_span)?;
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
}

impl rodio::Source for TestSource {
    fn parameters_changed(&self) -> bool {
        self.parameters_changed
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.spans
            .get(self.current_span)
            .map(|span| span.channels)
            .unwrap_or_default()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.spans
            .get(self.current_span)
            .map(|span| span.sample_rate)
            .unwrap_or_default()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
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
fn parameters_change_correct() {
    let mut source = TestSource::new()
        .with_span(TestSpan::silence(10))
        .with_span(TestSpan::silence(10));

    assert_eq!(source.by_ref().take(10).count(), 10);
    assert!(source.parameters_changed);

    assert!(source.next().is_some());
    assert!(!source.parameters_changed);

    assert_eq!(source.count(), 9);
}

#[test]
fn channel_count_changes() {
    let mut source = TestSource::new()
        .with_span(TestSpan::silence(10).with_channel_count(1))
        .with_span(TestSpan::silence(10).with_channel_count(2));

    assert_eq!(source.channels(), 1);
    assert_eq!(source.by_ref().take(10).count(), 10);
    assert_eq!(source.channels(), 2);
}

#[test]
fn sample_rate_changes() {
    let mut source = TestSource::new()
        .with_span(TestSpan::silence(10).with_sample_rate(10))
        .with_span(TestSpan::silence(10).with_sample_rate(20));

    assert_eq!(source.sample_rate(), 10);
    assert_eq!(source.by_ref().take(10).count(), 10);
    assert_eq!(source.sample_rate(), 20);
}

#[test]
fn sine_is_avg_zero() {
    let sine = TestSource::new().with_span(TestSpan::sine(400.0, 500).with_sample_rate(10_000));

    let avg = sine.clone().sum::<f32>() / sine.spans[0].len() as f32;
    assert!(avg < 0.00001f32);
}

#[test]
fn sine_abs_avg_not_zero() {
    let sine = TestSource::new().with_span(TestSpan::sine(400.0, 500).with_sample_rate(10_000));

    let avg = sine.clone().map(f32::abs).sum::<f32>() / sine.spans[0].len() as f32;
    assert!(avg > 0.5);
}
