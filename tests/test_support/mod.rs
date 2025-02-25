#![allow(dead_code)]
/// in separate folder so its not ran as integration test
/// should probably be moved to its own crate (rodio-test-support)
/// that would fix the unused code warnings.
use std::time::Duration;

use rodio::{ChannelCount, SampleRate, Source};

#[derive(Debug, Clone)]
pub struct TestSpan {
    pub data: Vec<f32>,
    pub sample_rate: SampleRate,
    pub channels: ChannelCount,
}

impl TestSpan {
    pub fn silence(numb_samples: usize) -> Self {
        Self {
            data: vec![0f32; numb_samples],
            sample_rate: 1,
            channels: 1,
        }
    }
    pub fn from_samples<'a>(samples: impl IntoIterator<Item = &'a f32>) -> Self {
        let samples = samples.into_iter().copied().collect::<Vec<f32>>();
        Self {
            data: samples,
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
    pub fn len(&self) -> usize {
        self.data.len()
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
        let sample = current_span.data.get(self.pos_in_span).copied()?;
        self.pos_in_span += 1;

        // if span is out of samples
        //  - next set parameters_changed now
        //  - switch to the next span
        if self.pos_in_span == current_span.data.len() {
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
    fn try_seek(&mut self, _pos: Duration) -> Result<(), rodio::source::SeekError> {
        todo!();
        // let duration_per_sample = Duration::from_secs(1) / self.sample_rate;
        // let offset = pos.div_duration_f64(duration_per_sample).floor() as usize;
        // self.pos = offset;

        Ok(())
    }
}

// test for your tests of course. Leave these in, they guard regression when we
// expand the functionally which we probably will.
#[test]
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
