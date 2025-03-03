use std::num::NonZero;
use std::time::Duration;

use rodio::buffer::SamplesBuffer;
use rodio::Source;

mod test_support;
use rstest::rstest;
use test_support::{TestSource, TestSpan};

#[rstest]
fn frame_changes(
    #[values(
        Duration::from_secs(0),
        Duration::from_secs_f32(4.8),
        Duration::from_secs(5),
        Duration::from_secs_f32(9.5),
        Duration::from_secs(12)
    )]
    to_skip: Duration,
) {
    // 5 seconds per span
    let source = TestSource::new()
        .with_span(
            TestSpan::silence()
                .with_sample_rate(100)
                .with_channel_count(2)
                .with_exact_duration(Duration::from_secs(5)),
        )
        .with_span(
            TestSpan::silence()
                .with_sample_rate(200)
                .with_channel_count(10)
                .with_exact_duration(Duration::from_secs(5)),
        )
        .with_span(
            TestSpan::silence()
                .with_sample_rate(50)
                .with_channel_count(1)
                .with_exact_duration(Duration::from_secs(5)),
        );

    let tracked = source.clone().track_position();
    let skipped = tracked.skip_duration(to_skip);
    let diff = to_skip
        .checked_sub(skipped.inner().get_pos())
        .expect("Should never report position beyond where we are")
        .as_secs_f32();

    let curr_span = (to_skip.as_secs_f32() / 5.0) as usize;
    let sample_rate = source.spans[curr_span].sample_rate as f32;
    assert!(diff < 1. / sample_rate) //
}

#[test]
fn basic_and_seek() {
    let inner = SamplesBuffer::new(
        NonZero::new(1).unwrap(),
        1,
        vec![10.0, -10.0, 10.0, -10.0, 20.0, -20.0],
    );
    let mut source = inner.track_position();

    assert_eq!(source.get_pos().as_secs_f32(), 0.0);
    source.next();
    assert_eq!(source.get_pos().as_secs_f32(), 1.0);

    source.next();
    assert_eq!(source.get_pos().as_secs_f32(), 2.0);

    assert_eq!(source.try_seek(Duration::new(1, 0)).is_ok(), true);
    assert_eq!(source.get_pos().as_secs_f32(), 1.0);
}

#[test]
fn basic_and_seek_in_presence_of_speedup() {
    let inner = SamplesBuffer::new(
        NonZero::new(1).unwrap(),
        1,
        vec![10.0, -10.0, 10.0, -10.0, 20.0, -20.0],
    );
    let mut source = inner.speed(2.0).track_position();

    assert_eq!(source.get_pos().as_secs_f32(), 0.0);
    source.next();
    assert_eq!(source.get_pos().as_secs_f32(), 0.5);

    source.next();
    assert_eq!(source.get_pos().as_secs_f32(), 1.0);

    assert_eq!(source.try_seek(Duration::new(1, 0)).is_ok(), true);
    assert_eq!(source.get_pos().as_secs_f32(), 1.0);
}
