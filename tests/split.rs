use rodio::Source;
use std::time::Duration;

struct TestSource {
    samples: Vec<f32>,
    pos: usize,
    channels: rodio::ChannelCount,
    sample_rate: rodio::SampleRate,
    total_duration: Option<Duration>,
}

impl TestSource {
    fn new<'a>(samples: impl IntoIterator<Item = &'a f32>) -> Self {
        Self {
            samples: samples.into_iter().copied().collect::<Vec<f32>>(),
            pos: 0,
            channels: 1,
            sample_rate: 1,
            total_duration: None,
        }
    }

    fn with_sample_rate(mut self, rate: rodio::SampleRate) -> Self {
        self.sample_rate = rate;
        self
    }
    fn with_channels(mut self, count: rodio::ChannelCount) -> Self {
        self.channels = count;
        self
    }
    fn with_total_duration(mut self, duration: Duration) -> Self {
        self.total_duration = Some(duration);
        self
    }
}

impl Iterator for TestSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let res = self.samples.get(self.pos).copied();
        self.pos += 1;
        res
    }
}

impl rodio::Source for TestSource {
    fn current_span_len(&self) -> Option<usize> {
        None // must be None or seek will not work
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.channels
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.sample_rate
    }
    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        let duration_per_sample = Duration::from_secs(1) / self.sample_rate;
        let offset = pos.div_duration_f64(duration_per_sample).floor() as usize;
        self.pos = offset;

        Ok(())
    }
}

#[test]
fn split_contains_all_samples() {
    let input = [0, 1, 2, 3, 4].map(|s| s as f32);
    let source = TestSource::new(&input)
        .with_channels(1)
        .with_sample_rate(1)
        .with_total_duration(Duration::from_secs(5));

    let [start, end] = source.split_once(Duration::from_secs(3));

    let played: Vec<_> = start.chain(end).collect();
    assert_eq!(input.as_slice(), played.as_slice());
}

#[test]
fn seek_over_segment_boundry() {
    let input = [0, 1, 2, 3, 4, 5, 6, 7].map(|s| s as f32);
    let source = TestSource::new(&input)
        .with_channels(1)
        .with_sample_rate(1)
        .with_total_duration(Duration::from_secs(5));

    let [mut start, mut end] = source.split_once(Duration::from_secs(3));
    assert_eq!(start.next(), Some(0.0));
    start.try_seek(Duration::from_secs(6)).unwrap();
    assert_eq!(end.next(), Some(6.0));
    assert_eq!(end.next(), Some(7.0));
}
