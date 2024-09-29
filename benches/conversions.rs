use cpal::FromSample;
use divan::Bencher;
use rodio::Source;

mod shared;
use shared::TestSource;

fn main() {
    divan::main();
}

#[divan::bench(types = [i16, u16, f32])]
fn from_i16_to<T: rodio::Sample + FromSample<i16>>(bencher: Bencher) {
    bencher
        .with_inputs(|| TestSource::music_wav())
        .bench_values(|source| {
            source
                .convert_samples::<T>()
                .for_each(divan::black_box_drop)
        })
}
