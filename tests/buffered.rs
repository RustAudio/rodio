mod test_support;
use rodio::Source;
use test_support::{TestSource, TestSpan};

#[test]
fn parameters_change_correct() {
    let mut source = TestSource::new()
        .with_span(TestSpan::silence().with_sample_count(10))
        .with_span(TestSpan::silence().with_sample_count(10))
        .buffered();

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
        )
        .buffered();

    assert_eq!(source.channels(), 1);
    assert_eq!(source.by_ref().take(10).count(), 10);
    assert_eq!(source.channels(), 2);
}

#[test]
fn buffered_sample_rate_changes() {
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
        )
        .buffered();

    assert_eq!(source.sample_rate(), 10);
    assert_eq!(source.by_ref().take(10).count(), 10);
    assert_eq!(source.sample_rate(), 20);
}

#[test]
fn equals_unbuffered() {
    let mut source = TestSource::new()
        .with_span(
            TestSpan::from_samples((0..10).into_iter().map(|n| n as f32))
                .with_sample_rate(10)
                .with_sample_count(10),
        )
        .with_span(
            TestSpan::silence()
                .with_sample_rate(20)
                .with_sample_count(10),
        );

    let mut buffered = source.clone().buffered();
    for (sample, expected) in buffered.by_ref().zip(source.by_ref()) {
        assert_eq!(sample, expected);
    }

    assert!(buffered.next().is_none());
    assert!(source.next().is_none());
}
