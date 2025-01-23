use std::time::Duration;

use rodio::buffer::SamplesBuffer;
use rodio::queue;
use rodio::source::Source;
use test_support::TestSource;

#[test]
// #[ignore] // FIXME: samples rate and channel not updated immediately after transition
fn basic() {
    let (controls, mut source) = queue::queue(false);

    controls.append(SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]));
    controls.append(SamplesBuffer::new(2, 96000, vec![5i16, 5, 5, 5]));

    assert_eq!(source.channels(), 1);
    assert_eq!(source.sample_rate(), 48000);
    assert_eq!(source.next(), Some(10));
    assert_eq!(source.next(), Some(-10));
    assert_eq!(source.next(), Some(10));
    assert_eq!(source.next(), Some(-10));
    assert_eq!(source.channels(), 2);
    assert_eq!(source.sample_rate(), 96000);
    assert_eq!(source.next(), Some(5));
    assert_eq!(source.next(), Some(5));
    assert_eq!(source.next(), Some(5));
    assert_eq!(source.next(), Some(5));
    assert_eq!(source.next(), None);
}

#[test]
fn immediate_end() {
    let (_, mut source) = queue::queue::<i16>(false);
    assert_eq!(source.next(), None);
}

#[test]
fn keep_alive() {
    let (controls, mut source) = queue::queue(true);
    controls.append(SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]));

    assert_eq!(source.next(), Some(10));
    assert_eq!(source.next(), Some(-10));
    assert_eq!(source.next(), Some(10));
    assert_eq!(source.next(), Some(-10));

    for _ in 0..100000 {
        assert_eq!(source.next(), Some(0));
    }
}

#[test]
fn limited_delay_when_added() {
    let (controls, mut source) = queue::queue(true);

    for _ in 0..500 {
        assert_eq!(source.next(), Some(0));
    }

    controls.append(SamplesBuffer::new(4, 41000, vec![10i16, -10, 10, -10]));
    let sample_rate = source.sample_rate() as f64;
    let channels = source.channels() as f64;
    let delay_samples = source.by_ref().take_while(|s| *s == 0).count();
    let delay = Duration::from_secs_f64(delay_samples as f64 / channels / sample_rate);
    assert!(delay < Duration::from_millis(5));

    // assert_eq!(source.next(), Some(10)); // we lose this in the take_while
    assert_eq!(source.next(), Some(-10));
    assert_eq!(source.next(), Some(10));
    assert_eq!(source.next(), Some(-10));
}

mod source_without_span_or_lower_bound_ending_early {
    use super::*;

    #[test]
    fn with_span_len_queried_before_source_end() {
        let test_source1 = TestSource::new(&[0.1; 5])
            .with_channels(1)
            .with_sample_rate(1)
            .with_false_span_len(None)
            .with_false_lower_bound(0);
        let test_source2 = TestSource::new(&[0.2; 5])
            .with_channels(1)
            .with_sample_rate(1);

        let (controls, mut source) = queue::queue(true);
        controls.append(test_source1);
        controls.append(test_source2);

        assert_eq!(source.current_span_len(), Some(200));
        assert_eq!(source.next(), Some(0.1));
        assert_eq!(source.next(), Some(0.1));
        assert_eq!(source.next(), Some(0.1));
        assert_eq!(source.next(), Some(0.1));
        assert_eq!(source.next(), Some(0.1));

        // silence filling the remaining fallback span
        assert_eq!(source.next(), Some(0.0));
    }

    #[test]
    fn without_span_queried() {
        let test_source1 = TestSource::new(&[0.1; 5])
            .with_channels(1)
            .with_sample_rate(1)
            .with_false_span_len(None)
            .with_false_lower_bound(0);
        let test_source2 = TestSource::new(&[0.2; 5])
            .with_channels(1)
            .with_sample_rate(1);

        let (controls, mut source) = queue::queue(true);
        controls.append(test_source1);
        controls.append(test_source2);

        assert_eq!(source.next(), Some(0.1));
        assert_eq!(source.next(), Some(0.1));
        assert_eq!(source.next(), Some(0.1));
        assert_eq!(source.next(), Some(0.1));
        assert_eq!(source.next(), Some(0.1));

        assert_eq!(source.current_span_len(), Some(5));
        assert_eq!(source.next(), Some(0.2));
    }
}

// should be made into its own crate called: `rodio-test-support`
mod test_support {
    use rodio::Source;
    use std::time::Duration;

    pub struct TestSource {
        samples: Vec<f32>,
        pos: usize,
        channels: rodio::ChannelCount,
        sample_rate: rodio::SampleRate,
        total_duration: Option<Duration>,
        lower_bound: usize,
        total_span_len: Option<usize>,
    }

    impl TestSource {
        pub fn new<'a>(samples: impl IntoIterator<Item = &'a f32>) -> Self {
            let samples = samples.into_iter().copied().collect::<Vec<f32>>();
            Self {
                pos: 0,
                channels: 1,
                sample_rate: 1,
                total_duration: None,
                lower_bound: samples.len(),
                total_span_len: Some(samples.len()),
                samples,
            }
        }

        pub fn with_sample_rate(mut self, rate: rodio::SampleRate) -> Self {
            self.sample_rate = rate;
            self
        }
        pub fn with_channels(mut self, count: rodio::ChannelCount) -> Self {
            self.channels = count;
            self
        }
        pub fn with_total_duration(mut self, duration: Duration) -> Self {
            self.total_duration = Some(duration);
            self
        }
        pub fn with_false_span_len(mut self, total_len: Option<usize>) -> Self {
            self.total_span_len = total_len;
            self
        }
        pub fn with_false_lower_bound(mut self, lower_bound: usize) -> Self {
            self.lower_bound = lower_bound;
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
        fn size_hint(&self) -> (usize, Option<usize>) {
            (self.lower_bound, None)
        }
    }

    impl rodio::Source for TestSource {
        fn current_span_len(&self) -> Option<usize> {
            self.total_span_len.map(|len| len.saturating_sub(self.pos))
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
}
