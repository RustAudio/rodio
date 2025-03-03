mod test_support;
use rodio::{SampleRate, Source};
use test_support::{TestSource, TestSpan};

use spectrum_analyzer::scaling::scale_to_zero_to_one;
use spectrum_analyzer::windows::hann_window;
use spectrum_analyzer::{samples_fft_to_spectrum, FrequencyLimit};

const SAMPLE_RATE: SampleRate = 44_100;

#[test]
fn low_pass_attenuates() {
    attenuate_test(
        |s: TestSource| s.low_pass(200),
        |ratio_before, ratio_after| {
            assert!(
                ratio_after < ratio_before / 10.0,
                "expected ratio between frequencies above freq (400) and below to have \
                decreased by at least a factor 10 after the low pass filter.\
                \n\tratio before: {ratio_before},\n\tratio_after: {ratio_after}"
            )
        },
    );
}

#[test]
fn high_pass_attenuates() {
    attenuate_test(
        |s: TestSource| s.high_pass(200),
        |ratio_before, ratio_after| {
            assert!(
                ratio_after > ratio_before * 4.0,
                "expected ratio between frequencies above freq (400) and below to have \
                increased by at least a factor 4 after the low pass filter.\
                \n\tratio before: {ratio_before},\n\tratio_after: {ratio_after}"
            )
        },
    );
}

fn attenuate_test<S: Source + Clone>(filter: impl Fn(TestSource) -> S, assert: impl Fn(f32, f32)) {
    let source = TestSource::new()
        .with_span(
            TestSpan::square(40.0)
                .with_sample_rate(SAMPLE_RATE)
                .with_channel_count(1)
                .with_sample_count(2048),
        )
        .with_span(
            TestSpan::square(40.0)
                .with_sample_rate(SAMPLE_RATE / 2)
                .with_channel_count(1)
                .with_sample_count(1024),
        );

    let span0_ratio_before = power_above_freq_vs_below(
        source.clone().take(source.spans[0].len()),
        400.0,
        source.spans[0].sample_rate,
    );
    let span1_ratio_before = power_above_freq_vs_below(
        source.clone().skip(source.spans[0].len()),
        400.0,
        source.spans[1].sample_rate,
    );

    let filterd = filter(source.clone());

    let span0_ratio_after = power_above_freq_vs_below(
        filterd.clone().take(source.spans[0].len()),
        400.0,
        source.spans[0].sample_rate,
    );
    let span1_ratio_after = power_above_freq_vs_below(
        filterd.skip(source.spans[0].len()),
        400.0,
        source.spans[1].sample_rate,
    );

    assert(span0_ratio_before, span0_ratio_after);
    assert(span1_ratio_before, span1_ratio_after);
}

fn power_above_freq_vs_below(
    source: impl Iterator<Item = f32>,
    split: f32,
    sample_rate: SampleRate,
) -> f32 {
    let samples: Vec<f32> = source.collect();
    let hann_window = hann_window(&samples);
    let spectrum = samples_fft_to_spectrum(
        &hann_window,
        sample_rate,
        FrequencyLimit::All,
        Some(&scale_to_zero_to_one),
    )
    .unwrap();

    let data = spectrum.data();
    let below: f32 = data
        .iter()
        .take_while(|(freq, _)| freq.val() < split)
        .map(|(_, val)| val.val())
        .sum();
    let above: f32 = data
        .iter()
        .skip_while(|(freq, _)| freq.val() < split)
        .map(|(_, val)| val.val())
        .sum();

    above / below
}
