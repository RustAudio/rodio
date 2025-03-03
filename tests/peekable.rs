mod test_support;
use rodio::Source;
use test_support::{TestSource, TestSpan};

#[test]
fn peeked_matches_next() {
    let source = TestSource::new()
        .with_span(
            TestSpan::from_samples((0..10).map(|n| n as f32))
                .with_sample_count(10),
        )
        .with_span(
            TestSpan::from_samples((10..20).map(|n| n as f32))
                .with_sample_count(10),
        );

    let mut peekable = source.peekable_source();

    for _ in 0..peekable.len() {
        let peeked = peekable.peek_next();
        let next = peekable.next();
        assert!(
            peeked == next,
            "peeked: {peeked:?} does not match following next: {next:?}"
        );
    }
}

#[test]
fn parameters_change_correct() {
    let source = TestSource::new()
        .with_span(TestSpan::silence().with_sample_count(10))
        .with_span(TestSpan::silence().with_sample_count(10));
    let mut peekable = source.peekable_source();
    peekable.peek_next();

    assert_eq!(peekable.by_ref().take(10).count(), 10);
    assert!(!peekable.parameters_changed());
    // end of first span

    assert!(peekable.next().is_some());
    assert!(peekable.parameters_changed());

    assert_eq!(peekable.count(), 9);
}

#[test]
fn channel_count_changes() {
    let source = TestSource::new()
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
    let mut peekable = source.peekable_source();
    peekable.peek_next();

    assert_eq!(peekable.channels().get(), 1);
    assert_eq!(peekable.by_ref().take(10).count(), 10);
    assert_eq!(peekable.channels().get(), 2);
}

#[test]
fn sample_rate_changes() {
    let source = TestSource::new()
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
    let mut peekable = source.peekable_source();
    peekable.peek_next();

    assert_eq!(peekable.sample_rate(), 10);
    assert_eq!(peekable.by_ref().take(10).count(), 10);
    assert_eq!(peekable.sample_rate(), 20);
}
