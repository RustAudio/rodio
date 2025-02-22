use dasp_sample::{Duplex, Sample};
use divan::Bencher;
use rodio::conversions::SampleTypeConverter;

mod shared;

fn main() {
    divan::main();
}

#[divan::bench(types = [i16, u16, f32])]
fn from_sample<S: Duplex<f32>>(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            shared::music_wav()
                .map(|s| s.to_sample::<S>())
                .collect::<Vec<_>>()
                .into_iter()
        })
        .bench_values(|source| {
            SampleTypeConverter::<_, rodio::Sample>::new(source).for_each(divan::black_box_drop)
        })
}
