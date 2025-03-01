use rodio::queue::Queue;
use rodio::source::Source;

mod test_support;
use test_support::{TestSource, TestSpan};

#[test]
fn basic() {
    let (controls, mut source) = Queue::new(false);

    let mut source1 = TestSource::new().with_span(
        TestSpan::silence()
            .with_sample_rate(48000)
            .with_sample_count(4),
    );
    let mut source2 = TestSource::new().with_span(
        TestSpan::silence()
            .with_sample_rate(96000)
            .with_channel_count(2)
            .with_sample_count(4),
    );
    controls.append(source1.clone());
    controls.append(source2.clone());

    assert_eq!(source.parameters_changed(), true);
    assert_eq!(source.channels(), source1.channels());
    assert_eq!(source.sample_rate(), source1.sample_rate());
    assert_eq!(source.next(), source1.next());
    assert_eq!(source.next(), source1.next());
    assert_eq!(source.next(), source1.next());
    assert_eq!(source.next(), source1.next());
    assert_eq!(None, source1.next());
    assert_eq!(source.parameters_changed(), true);

    assert_eq!(source.parameters_changed(), true);
    assert_eq!(source.channels(), source2.channels());
    assert_eq!(source.sample_rate(), source2.sample_rate());
    assert_eq!(source.next(), source2.next());
    assert_eq!(source.next(), source2.next());
    assert_eq!(source.parameters_changed(), false);
    assert_eq!(source.next(), source2.next());
    assert_eq!(source.next(), source2.next());
    assert_eq!(None, source2.next());

    assert_eq!(source.next(), None);
}

#[test]
fn immediate_end() {
    let (_, mut source) = Queue::new(false);
    assert_eq!(source.next(), None);
}

#[test]
fn keep_alive() {
    let (controls, mut source) = Queue::new(true);
    controls.append(
        TestSource::new()
            .with_span(TestSpan::from_samples([0.1, -0.1, 0.1, -0.1]).with_sample_count(4)),
    );

    assert_eq!(source.next(), Some(0.1));
    assert_eq!(source.next(), Some(-0.1));
    assert_eq!(source.next(), Some(0.1));
    assert_eq!(source.next(), Some(-0.1));

    for _ in 0..1000 {
        assert_eq!(source.next(), Some(0.0));
    }
}

#[test]
fn no_delay_when_added_with_keep_alive() {
    let (controls, mut source) = Queue::new(true);

    for _ in 0..500 {
        assert_eq!(source.next(), Some(0.0));
    }

    controls.append(
        TestSource::new().with_span(
            TestSpan::from_samples([0.1, -0.1, 0.1, -0.1])
                .with_channel_count(4)
                .with_sample_count(4),
        ),
    );

    assert_eq!(source.next(), Some(0.1));
    assert_eq!(source.next(), Some(-0.1));
    assert_eq!(source.next(), Some(0.1));
    assert_eq!(source.next(), Some(-0.1));
}

#[test]
fn parameters_queried_before_next() {
    let test_source =
        TestSource::new().with_span(TestSpan::ones().with_channel_count(5).with_sample_count(20));

    let (controls, mut source) = Queue::new(true);

    assert_eq!(source.channels().get(), 1);
    controls.append(test_source);
    assert_eq!(source.channels().get(), 5);
    assert_eq!(source.next(), Some(1.0));
}
