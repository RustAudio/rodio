use crate::buffer::SamplesBuffer;
use crate::math::nz;
use crate::mixer;
use crate::source::Source;

#[test]
fn basic() {
    let (tx, mut rx) = mixer::mixer(nz!(1), nz!(48000));

    tx.add(SamplesBuffer::new(
        nz!(1),
        nz!(48000),
        vec![10.0, -10.0, 10.0, -10.0],
    ));
    tx.add(SamplesBuffer::new(
        nz!(1),
        nz!(48000),
        vec![5.0, 5.0, 5.0, 5.0],
    ));

    assert_eq!(rx.channels(), nz!(1));
    assert_eq!(rx.sample_rate().get(), 48000);
    assert_eq!(rx.next(), Some(15.0));
    assert_eq!(rx.next(), Some(-5.0));
    assert_eq!(rx.next(), Some(15.0));
    assert_eq!(rx.next(), Some(-5.0));
    assert_eq!(rx.next(), None);
}

#[test]
fn channels_conv() {
    let (tx, mut rx) = mixer::mixer(nz!(2), nz!(48000));

    tx.add(SamplesBuffer::new(
        nz!(1),
        nz!(48000),
        vec![10.0, -10.0, 10.0, -10.0],
    ));
    tx.add(SamplesBuffer::new(
        nz!(1),
        nz!(48000),
        vec![5.0, 5.0, 5.0, 5.0],
    ));

    assert_eq!(rx.channels(), nz!(2));
    assert_eq!(rx.sample_rate().get(), 48000);
    assert_eq!(rx.next(), Some(15.0));
    assert_eq!(rx.next(), Some(15.0));
    assert_eq!(rx.next(), Some(-5.0));
    assert_eq!(rx.next(), Some(-5.0));
    assert_eq!(rx.next(), Some(15.0));
    assert_eq!(rx.next(), Some(15.0));
    assert_eq!(rx.next(), Some(-5.0));
    assert_eq!(rx.next(), Some(-5.0));
    assert_eq!(rx.next(), None);
}

#[test]
fn rate_conv() {
    let (tx, mut rx) = mixer::mixer(nz!(1), nz!(96000));

    tx.add(SamplesBuffer::new(
        nz!(1),
        nz!(48000),
        vec![10.0, -10.0, 10.0, -10.0],
    ));
    tx.add(SamplesBuffer::new(
        nz!(1),
        nz!(48000),
        vec![5.0, 5.0, 5.0, 5.0],
    ));

    assert_eq!(rx.channels(), nz!(1));
    assert_eq!(rx.sample_rate().get(), 96000);
    assert_eq!(rx.next(), Some(15.0));
    assert_eq!(rx.next(), Some(5.0));
    assert_eq!(rx.next(), Some(-5.0));
    assert_eq!(rx.next(), Some(5.0));
    assert_eq!(rx.next(), Some(15.0));
    assert_eq!(rx.next(), Some(5.0));
    assert_eq!(rx.next(), Some(-5.0));
    assert_eq!(rx.next(), Some(-2.5));
    assert_eq!(rx.next(), None);
}

#[test]
fn start_afterwards() {
    let (tx, mut rx) = mixer::mixer(nz!(1), nz!(48000));

    tx.add(SamplesBuffer::new(
        nz!(1),
        nz!(48000),
        vec![10.0, -10.0, 10.0, -10.0],
    ));

    assert_eq!(rx.next(), Some(10.0));
    assert_eq!(rx.next(), Some(-10.0));

    tx.add(SamplesBuffer::new(
        nz!(1),
        nz!(48000),
        vec![5.0, 5.0, 6.0, 6.0, 7.0, 7.0, 7.0],
    ));

    assert_eq!(rx.next(), Some(15.0)); // 10 + 5
    assert_eq!(rx.next(), Some(-5.0));

    assert_eq!(rx.next(), Some(6.0));
    assert_eq!(rx.next(), Some(6.0));

    tx.add(SamplesBuffer::new(nz!(1), nz!(48000), vec![2.0]));

    assert_eq!(rx.next(), Some(9.0));
    assert_eq!(rx.next(), Some(7.0));
    assert_eq!(rx.next(), Some(7.0));

    assert_eq!(rx.next(), None);
}

#[test]
fn added_taking_phase_into_account() {
    let (tx, mut rx) = mixer::mixer(nz!(2), nz!(48000));

    tx.add(SamplesBuffer::new(
        nz!(2),
        nz!(48000),
        vec![10.0, -10.0, 10.0, -10.0],
    ));

    assert_eq!(rx.next(), Some(10.0));

    tx.add(SamplesBuffer::new(
        nz!(2),
        nz!(48000),
        vec![5.0, -5.0, 6.0, -6.0],
    ));

    assert_eq!(rx.next(), Some(-10.0)); // not yet mixed (out of phase)
    assert_eq!(rx.next(), Some(15.0)); // mixing starts
}

/// Buffers played one after the other, as `(frames, rate, channels)`.
fn spans(spans: &[(usize, u32, u16)]) -> impl Source + Send {
    let buffers: Vec<_> = spans
        .iter()
        .map(|&(frames, rate, channels)| {
            let samples = vec![0.25; frames * channels as usize];
            SamplesBuffer::new(
                channels.try_into().unwrap(),
                rate.try_into().unwrap(),
                samples,
            )
        })
        .collect();
    crate::source::chain(buffers)
}

/// A resampler reads ahead, so it has moved on to the next format while
/// samples of the last one are still to come. Those must still play.
#[test]
fn format_change_keeps_resampled_samples() {
    for (spans_in, mixer_channels, expected) in [
        // Read up to the 48 kHz span with 24 kHz samples still buffered.
        (
            &[(128, 24000, 1), (64, 24000, 1), (96, 48000, 1)][..],
            1,
            480,
        ),
        // Same, when the next span also changes the channel count.
        (&[(300, 24000, 1), (200, 44100, 2)], 2, 1200 + 2 * 218),
        // Downmixing as well as resampling.
        (&[(300, 24000, 2), (200, 24000, 1)], 1, 600 + 400),
    ] {
        let (tx, rx) = mixer::mixer(mixer_channels.try_into().unwrap(), nz!(48000));
        tx.add(spans(spans_in));
        let played = rx.count();
        assert!(
            played.abs_diff(expected) <= 16,
            "{spans_in:?}: played {played} samples, expected {expected}"
        );
    }
}
