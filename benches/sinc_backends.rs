//! Apples-to-apples fixed-ratio Sinc benchmark.
//!
//! Run this benchmark twice with the same release profile:
//!
//! ```text
//! cargo bench --bench sinc_backends --no-default-features
//! cargo bench --bench sinc_backends --no-default-features --features fixed-fir
//! ```
//!
//! `with_inputs` keeps source generation out of the timed loop. Converter construction (including
//! plan setup) is inside the timed operation for both backends. The input, ratio, channels,
//! duration, and configuration are identical; the feature selects only the backend.

use divan::Bencher;
use rodio::conversions::SampleRateConverter;
use rodio::source::{from_iter, FromIter, ResampleConfig};
use rodio::{ChannelCount, Sample, SampleRate};

fn main() {
    divan::main();
}

fn input(channels: ChannelCount) -> FromIter<std::vec::IntoIter<Sample>> {
    let frames = 44_100 * 2;
    let channels = channels.get() as usize;
    let samples = (0..frames * channels)
        .map(|i| {
            let frame = i / channels;
            let channel = i % channels;
            (2.0 * std::f32::consts::PI * (440.0 + 37.0 * channel as f32) * frame as f32 / 44_100.0)
                .sin()
                * 0.5
        })
        .collect::<Vec<_>>();
    from_iter(
        samples.into_iter(),
        ChannelCount::new(channels as u16).unwrap(),
        SampleRate::new(44_100).unwrap(),
    )
}

#[divan::bench(args = [1u16, 2u16])]
fn sinc_44100_to_48000(bencher: Bencher, channel_count: u16) {
    let channels: ChannelCount = channel_count.try_into().unwrap();
    bencher
        .with_inputs(|| input(channels))
        .bench_values(|source| {
            SampleRateConverter::new(
                source,
                SampleRate::new(48_000).unwrap(),
                ResampleConfig::balanced(),
            )
            .for_each(divan::black_box_drop)
        });
}
