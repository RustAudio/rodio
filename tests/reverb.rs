mod test_support;
use std::time::Duration;

use rodio::Source;
use test_support::{TestSource, TestSpan};

#[test]
fn without_buffered() {
    let source = TestSource::new()
        .with_span(TestSpan::silence().with_exact_duration(Duration::from_secs(5)));

    let reverb = source.clone().reverb(Duration::from_secs_f32(0.05), 0.3);
    let n_samples = reverb.count();

    assert_eq!(n_samples, source.len(),);
}

#[test]
fn with_buffered() {
    let source = TestSource::new()
        .with_span(TestSpan::silence().with_exact_duration(Duration::from_secs(5)));

    let reverb = source.clone().buffered().reverb(Duration::from_secs_f32(0.05), 0.3);
    let n_samples = reverb.count();

    assert_eq!(n_samples, source.len(),);
}
