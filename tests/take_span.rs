use rodio::Source;
use test_support::{TestSource, TestSpan};

mod test_support;

#[test]
fn param_changes_during_skip() {
    let source = TestSource::new()
        .with_span(
            TestSpan::sample_indexes()
                .with_sample_rate(10)
                .with_channel_count(1)
                .with_sample_count(10),
        )
        .with_span(
            TestSpan::sample_indexes()
                .with_sample_rate(20)
                .with_channel_count(2)
                .with_sample_count(10),
        );

    let mut span_1 = source.take_span();
    assert_eq!(span_1.by_ref().take(9).count(), 9);
    assert_eq!(span_1.channels(), 1);
    assert_eq!(span_1.sample_rate(), 10);
    assert!(span_1.by_ref().next().is_some());
    assert_eq!(span_1.by_ref().next(), None);

    let mut span_2 = span_1.into_inner().take_span();
    assert_eq!(span_2.by_ref().take(9).count(), 9);
    assert_eq!(span_2.channels(), 2);
    assert_eq!(span_2.sample_rate(), 20);
    assert!(span_2.by_ref().next().is_some());
    assert_eq!(span_2.by_ref().next(), None);
}
