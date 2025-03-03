use std::num::NonZero;
use std::time::Duration;

use rodio::buffer::SamplesBuffer;
use rodio::source::Zero;
use rodio::Source;

fn dummy_source(length: u8) -> SamplesBuffer {
    let data: Vec<f32> = (1..=length).map(f32::from).collect();
    SamplesBuffer::new(NonZero::new(1).unwrap(), 1, data)
}

#[test]
fn test_crossfade_with_self() {
    let source1 = dummy_source(10);
    let source2 = dummy_source(10);
    let mut mixed =
        source1.take_crossfade_with(source2, Duration::from_secs(5) + Duration::from_nanos(1));
    assert_eq!(mixed.next(), Some(1.0));
    assert_eq!(mixed.next(), Some(2.0));
    assert_eq!(mixed.next(), Some(3.0));
    assert_eq!(mixed.next(), Some(4.0));
    assert_eq!(mixed.next(), Some(5.0));
    assert_eq!(mixed.next(), None);
}

#[test]
fn test_crossfade() {
    let source1 = dummy_source(10);
    let source2 = Zero::new(NonZero::new(1).unwrap(), 1);
    let mixed =
        source1.take_crossfade_with(source2, Duration::from_secs(5) + Duration::from_nanos(1));
    let result = mixed.collect::<Vec<_>>();
    assert_eq!(result.len(), 5);
    assert!(result
        .iter()
        .zip(vec![1.0, 2.0 * 0.8, 3.0 * 0.6, 4.0 * 0.4, 5.0 * 0.2])
        .all(|(a, b)| (a - b).abs() < 1e-6));
}
