use rodio::conversions::ChannelCountConverter;
use rodio::source::Zero;

#[test]
fn unbounded_mono_source_can_be_mixed_into_stereo() {
    let (mixer, mut source) =
        rodio::mixer::mixer(2.try_into().unwrap(), 48_000.try_into().unwrap());
    mixer.add(Zero::new(1.try_into().unwrap(), 48_000.try_into().unwrap()));

    for _ in 0..8 {
        assert_eq!(source.next(), Some(0.0));
        let (lower, upper) = source.size_hint();
        assert!(lower > 0);
        assert_eq!(upper, None);
    }
}

#[test]
fn unbounded_channel_conversion_hints_do_not_overflow() {
    for (from, to) in [(1, 2), (2, 4), (4, 2)] {
        let mut source = ChannelCountConverter::new(
            std::iter::repeat(0.0),
            from.try_into().unwrap(),
            to.try_into().unwrap(),
        );
        for _ in 0..8 {
            let (lower, upper) = source.size_hint();
            assert!(lower > 0);
            assert_eq!(upper, None);
            assert_eq!(source.next(), Some(0.0));
        }
    }
}

#[test]
fn overflowing_finite_upper_bound_becomes_unknown() {
    let mut source = ChannelCountConverter::new(
        std::iter::repeat_n(0.0, usize::MAX),
        1.try_into().unwrap(),
        2.try_into().unwrap(),
    );
    for _ in 0..8 {
        assert_eq!(source.size_hint().1, None);
        assert_eq!(source.next(), Some(0.0));
    }
}
