use std::time::Duration;

use rodio::buffer::SamplesBuffer;
use rodio::source::Source;
use rodio::{ChannelCount, SampleRate};
use rstest::rstest;

mod test_support;
use test_support::{TestSource, TestSpan};

#[rstest]
fn ends_on_frame_boundary(#[values(1.483, 2.999)] seconds_to_skip: f32) {
    let source = TestSource::new().with_span(
        TestSpan::silence()
            .with_channel_count(10)
            .with_sample_rate(1)
            .with_exact_duration(Duration::from_secs(3)),
    );
    let got = source
        .clone()
        .take_duration(Duration::from_secs_f32(seconds_to_skip))
        .count();
    assert!(got % source.channels() as usize == 0)
}

#[rstest]
fn param_changes_during_skip(#[values(6, 11)] seconds_to_skip: u64) {
    let span_duration = Duration::from_secs(5);
    let source = TestSource::new()
        .with_span(
            TestSpan::silence()
                .with_sample_rate(10)
                .with_channel_count(1)
                .with_exact_duration(span_duration),
        )
        .with_span(
            TestSpan::silence()
                .with_sample_rate(20)
                .with_channel_count(2)
                .with_exact_duration(span_duration),
        )
        .with_span(
            TestSpan::silence()
                .with_sample_rate(5)
                .with_channel_count(3)
                .with_exact_duration(span_duration),
        );

    let took = source
        .clone()
        .take_duration(Duration::from_secs(seconds_to_skip))
        .count();

    let spans = source.spans;
    let expected = match seconds_to_skip {
        6 => spans[0].len() + 1 * spans[1].sample_rate as usize * spans[1].channels as usize,
        11 => {
            spans[0].len()
                + spans[1].len()
                + 1 * spans[2].sample_rate as usize * spans[2].channels as usize
        }
        _ => unreachable!(),
    };

    assert!(
        took == expected,
        "expected {expected} samples, took only: {took}"
    );
}

#[rstest]
fn samples_taken(
    #[values(1, 2, 4)] channels: ChannelCount,
    #[values(100_000)] sample_rate: SampleRate,
    #[values(0, 5)] seconds: u32,
    #[values(0, 3, 5)] seconds_to_take: u32,
) {
    println!(
        "channels: {channels}, sample_rate: {sample_rate}, \
        seconds: {seconds}, seconds_to_take: {seconds_to_take}"
    );
    let buf_len = (sample_rate * channels as u32 * seconds) as usize;
    assert!(buf_len < 10 * 1024 * 1024);
    let data: Vec<f32> = vec![0f32; buf_len];
    let test_buffer = SamplesBuffer::new(channels, sample_rate, data);

    let samples = test_buffer
        .take_duration(Duration::from_secs(seconds_to_take as u64))
        .count();

    let seconds_we_can_take = seconds_to_take.min(seconds);
    let samples_expected = (sample_rate * channels as u32 * seconds_we_can_take) as usize;
    assert!(
        samples == samples_expected,
        "expected {samples_expected} samples, took only: {samples}"
    );
}
