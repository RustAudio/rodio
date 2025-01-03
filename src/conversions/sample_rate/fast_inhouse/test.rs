use super::SampleRateConverter;
use core::time::Duration;
use cpal::{ChannelCount, SampleRate};
use quickcheck::TestResult;
use quickcheck_macros::quickcheck;


/// Check that resampling an empty input produces no output.
#[quickcheck]
fn empty(from: u16, to: u16, channels: u8) -> TestResult {
    if channels == 0 || channels > 128 || from == 0 || to == 0 {
        return TestResult::discard();
    }
    let from = SampleRate(from as u32);
    let to = SampleRate(to as u32);

    let input: Vec<u16> = Vec::new();
    let output = SampleRateConverter::new(input.into_iter(), from, to, channels as ChannelCount)
        .collect::<Vec<_>>();

    assert_eq!(output, []);
    TestResult::passed()
}

/// Check that resampling to the same rate does not change the signal.
#[quickcheck]
fn identity(from: u16, channels: u8, input: Vec<u16>) -> TestResult {
    if channels == 0 || channels > 128 || from == 0 {
        return TestResult::discard();
    }
    let from = SampleRate(from as u32);

    let output = SampleRateConverter::new(
        input.clone().into_iter(),
        from,
        from,
        channels as ChannelCount,
    )
    .collect::<Vec<_>>();

    TestResult::from_bool(input == output)
}

/// Check that dividing the sample rate by k (integer) is the same as
///   dropping a sample from each channel.
#[quickcheck]
fn divide_sample_rate(to: u16, k: u16, input: Vec<u16>, channels: u8) -> TestResult {
    if k == 0 || channels == 0 || channels > 128 || to == 0 || to > 48000 {
        return TestResult::discard();
    }

    let to = SampleRate(to as u32);
    let from = to * k as u32;

    // Truncate the input, so it contains an integer number of frames.
    let input = {
        let ns = channels as usize;
        let mut i = input;
        i.truncate(ns * (i.len() / ns));
        i
    };

    let output = SampleRateConverter::new(
        input.clone().into_iter(),
        from,
        to,
        channels as ChannelCount,
    )
    .collect::<Vec<_>>();

    TestResult::from_bool(
        input
            .chunks_exact(channels.into())
            .step_by(k as usize)
            .collect::<Vec<_>>()
            .concat()
            == output,
    )
}

/// Check that, after multiplying the sample rate by k, every k-th
///  sample in the output matches exactly with the input.
#[quickcheck]
fn multiply_sample_rate(from: u16, k: u8, input: Vec<u16>, channels: u8) -> TestResult {
    if k == 0 || channels == 0 || channels > 128 || from == 0 {
        return TestResult::discard();
    }

    let from = SampleRate(from as u32);
    let to = from * k as u32;

    // Truncate the input, so it contains an integer number of frames.
    let input = {
        let ns = channels as usize;
        let mut i = input;
        i.truncate(ns * (i.len() / ns));
        i
    };

    let output = SampleRateConverter::new(
        input.clone().into_iter(),
        from,
        to,
        channels as ChannelCount,
    )
    .collect::<Vec<_>>();

    TestResult::from_bool(
        input
            == output
                .chunks_exact(channels.into())
                .step_by(k as usize)
                .collect::<Vec<_>>()
                .concat(),
    )
}

#[ignore]
/// Check that resampling does not change the audio duration,
///  except by a negligible amount (± 1ms). Reproduces #316.
/// Ignored, pending a bug fix.
#[quickcheck]
fn preserve_durations(d: Duration, freq: f32, to: u32) -> TestResult {
    if to == 0 {
        return TestResult::discard();
    }

    use crate::source::{SineWave, Source};

    let to = SampleRate(to);
    let source = SineWave::new(freq).take_duration(d);
    let from = SampleRate(source.sample_rate());

    let resampled = SampleRateConverter::new(source, from, to, 1);
    let duration = Duration::from_secs_f32(resampled.count() as f32 / to.0 as f32);

    let delta = if d < duration {
        duration - d
    } else {
        d - duration
    };
    TestResult::from_bool(delta < Duration::from_millis(1))
}

#[test]
fn upsample() {
    let input = vec![2u16, 16, 4, 18, 6, 20, 8, 22];
    let output = SampleRateConverter::new(input.into_iter(), SampleRate(2000), SampleRate(3000), 2);
    assert_eq!(output.len(), 12); // Test the source's Iterator::size_hint()

    let output = output.collect::<Vec<_>>();
    assert_eq!(output, [2, 16, 3, 17, 4, 18, 6, 20, 7, 21, 8, 22]);
}

#[test]
fn upsample2() {
    let input = vec![1u16, 14];
    let output = SampleRateConverter::new(input.into_iter(), SampleRate(1000), SampleRate(7000), 1);
    let size_estimation = output.len();
    let output = output.collect::<Vec<_>>();
    assert_eq!(output, [1, 2, 4, 6, 8, 10, 12, 14]);
    assert!((size_estimation as f32 / output.len() as f32).abs() < 2.0);
}

#[test]
fn downsample() {
    let input = Vec::from_iter(0u16..17);
    let output =
        SampleRateConverter::new(input.into_iter(), SampleRate(12000), SampleRate(2400), 1);
    let size_estimation = output.len();
    let output = output.collect::<Vec<_>>();
    assert_eq!(output, [0, 5, 10, 15]);
    assert!((size_estimation as f32 / output.len() as f32).abs() < 2.0);
}
