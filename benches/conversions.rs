use dasp_sample::FromSample;
use divan::Bencher;
use rodio::{decoder::DecoderSample, Source};

mod shared;

fn main() {
    divan::main();
}

// #[divan::bench(types = [i16, u16, f32])]
// fn from_sample_to(bencher: Bencher) {
//     bencher
//         .with_inputs(|| shared::music_wav())
//         .bench_values(|source| source.for_each(divan::black_box_drop))
// }
