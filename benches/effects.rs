use std::time::Duration;

use divan::Bencher;
use rodio::Source;

mod shared;
use shared::TestSource;

fn main() {
    divan::main();
}

#[divan::bench]
fn reverb(bencher: Bencher) {
    bencher
        .with_inputs(|| TestSource::music_wav())
        .bench_values(|source| {
            source
                .buffered()
                .reverb(Duration::from_secs_f32(0.05), 0.3)
                .for_each(divan::black_box_drop)
        })
}

#[divan::bench]
fn high_pass(bencher: Bencher) {
    bencher
        .with_inputs(|| TestSource::music_wav().to_f32s())
        .bench_values(|source| source.high_pass(200).for_each(divan::black_box_drop))
}

#[divan::bench]
fn fade_out(bencher: Bencher) {
    bencher
        .with_inputs(|| TestSource::music_wav())
        .bench_values(|source| {
            source
                .fade_out(Duration::from_secs(5))
                .for_each(divan::black_box_drop)
        })
}

#[divan::bench]
fn amplify(bencher: Bencher) {
    bencher
        .with_inputs(|| TestSource::music_wav().to_f32s())
        .bench_values(|source| source.amplify(0.8).for_each(divan::black_box_drop))
}

#[divan::bench]
fn agc_enabled(bencher: Bencher) {
    bencher
        .with_inputs(|| TestSource::music_wav().to_f32s())
        .bench_values(|source| {
            source
                .automatic_gain_control(
                    1.0,   // target_level
                    4.0,   // attack_time (in seconds)
                    0.005, // release_time (in seconds)
                    5.0,   // absolute_max_gain
                )
                .for_each(divan::black_box_drop)
        })
}

#[cfg(feature = "experimental")]
#[divan::bench]
fn agc_disabled(bencher: Bencher) {
    bencher
        .with_inputs(|| TestSource::music_wav().to_f32s())
        .bench_values(|source| {
            // Create the AGC source
            let amplified_source = source.automatic_gain_control(
                1.0,   // target_level
                4.0,   // attack_time (in seconds)
                0.005, // release_time (in seconds)
                5.0,   // absolute_max_gain
            );

            // Get the control handle and disable AGC
            let agc_control = amplified_source.get_agc_control();
            agc_control.store(false, std::sync::atomic::Ordering::Relaxed);

            // Process the audio stream with AGC disabled
            amplified_source.for_each(divan::black_box_drop)
        })
}
