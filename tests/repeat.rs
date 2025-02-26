use rodio::Source;
use test_support::{TestSource, TestSpan};

mod test_support;

#[test]
fn frame_boundray_at_start_of_repeat() {
    let source = TestSource::new()
        .with_span(TestSpan::from_samples((0..10).map(|n| n as f32)).with_sample_count(10))
        .with_span(TestSpan::from_samples((10..20).map(|n| n as f32)).with_sample_count(10));

    let mut repeating = source.clone().repeat_infinite();
    repeating.by_ref().take(source.len()).count();
    assert!(repeating.parameters_changed());

    assert!(repeating.next().is_some());
    assert!(!repeating.parameters_changed());
}

#[test]
fn parameters_identical_on_second_run() {
    let source = TestSource::new()
        .with_span(TestSpan::from_samples((0..10).map(|n| n as f32)).with_sample_count(10))
        .with_span(TestSpan::from_samples((10..20).map(|n| n as f32)).with_sample_count(10));

    let mut repeating = source.clone().repeat_infinite();

    let mut first_run_params = Vec::new();
    let mut second_run_params = Vec::new();

    for params in [&mut first_run_params, &mut second_run_params] {
        for _ in 0..source.len() {
            assert!(repeating.by_ref().next().is_some());
            params.push((
                repeating.parameters_changed(),
                repeating.channels(),
                repeating.sample_rate(),
            ));
        }
    }

    assert_eq!(first_run_params, second_run_params);
}

#[test]
fn same_samples_on_second_run() {
    let source = TestSource::new()
        .with_span(TestSpan::from_samples((0..10).map(|n| n as f32)).with_sample_count(10))
        .with_span(TestSpan::from_samples((10..20).map(|n| n as f32)).with_sample_count(10));

    let mut repeating = source.clone().repeat_infinite();
    let first_run: Vec<_> = repeating.by_ref().take(source.len()).collect();
    let second_run: Vec<_> = repeating.take(source.len()).collect();

    assert_eq!(first_run, second_run);
}
