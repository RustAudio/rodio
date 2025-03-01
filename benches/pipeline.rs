use std::time::Duration;

use divan::Bencher;
use rodio::{source::UniformSourceIterator, Source};

mod shared;
use shared::music_wav;

fn main() {
    divan::main();
}

#[divan::bench]
fn long(bencher: Bencher) {
    bencher.with_inputs(|| music_wav()).bench_values(|source| {
        let effects_applied = source
            .high_pass(300)
            .amplify(1.2)
            .speed(0.9)
            .automatic_gain_control(
                1.0,   // target_level
                4.0,   // attack_time (in seconds)
                0.005, // release_time (in seconds)
                5.0,   // absolute_max_gain
            )
            .delay(Duration::from_secs_f32(0.5))
            .fade_in(Duration::from_secs_f32(2.0))
            .take_duration(Duration::from_secs(10))
            .with_fadeout(true)
            .buffered()
            .reverb(Duration::from_secs_f32(0.05), 0.3)
            .skippable();
        let resampled = UniformSourceIterator::new(effects_applied, 2, 40_000);
        resampled.for_each(divan::black_box_drop)
    })
}

#[divan::bench]
fn short(bencher: Bencher) {
    bencher.with_inputs(|| music_wav()).bench_values(|source| {
        source
            .amplify(1.2)
            .low_pass(200)
            .for_each(divan::black_box_drop)
    })
}
