use rodio::{buffer::SamplesBuffer, Source};
use std::time::Duration;

#[test]
fn split_contains_all_samples() {
    let input = [0, 1, 2, 3, 4].map(|s| s as f32);
    let source = SamplesBuffer::new(1, 1, input);

    let [start, end] = source.split_once(Duration::from_secs(3));

    let played: Vec<_> = start.chain(end).collect();
    assert_eq!(input.as_slice(), played.as_slice());
}

#[test]
fn seek_over_segment_boundry() {
    let input = [0, 1, 2, 3, 4, 5, 6, 7].map(|s| s as f32);
    let source = SamplesBuffer::new(1, 1, input);

    let [mut start, mut end] = source.split_once(Duration::from_secs(3));
    assert_eq!(start.next(), Some(0.0));
    start.try_seek(Duration::from_secs(6)).unwrap();
    assert_eq!(end.next(), Some(6.0));
    assert_eq!(end.next(), Some(7.0));
}
