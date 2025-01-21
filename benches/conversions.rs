use dasp_sample::FromSample;
use divan::Bencher;
use rodio::{decoder::DecoderSample, Source};

mod shared;

fn main() {
    divan::main();
}

#[divan::bench(types = [i16, u16, f32])]
fn from_sample_to<T: rodio::Sample + FromSample<DecoderSample>>(bencher: Bencher) {
    bencher
        .with_inputs(|| shared::music_wav())
        .bench_values(|source| {
            source
                .convert_samples::<T>()
                .for_each(divan::black_box_drop)
        })
}
