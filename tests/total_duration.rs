#![allow(dead_code)]
#![allow(unused_imports)]

use std::io::{Read, Seek};
use std::num::NonZero;
use std::path::Path;
use std::time::Duration;

use rodio::source::{SineWave, Source};
use rodio::Decoder;

use rstest::rstest;
use rstest_reuse::{self, *};

#[cfg(any(
    feature = "claxon",
    feature = "minimp3",
    feature = "symphonia-aac",
    feature = "symphonia-flac",
    feature = "symphonia-mp3",
    feature = "symphonia-isomp4",
    feature = "symphonia-ogg",
    feature = "symphonia-wav",
    feature = "hound",
))]
#[template]
#[rstest]
#[cfg_attr(
    feature = "symphonia-vorbis",
    case("ogg", Duration::from_secs_f64(69.328979591), "symphonia")
)]
#[cfg_attr(
    all(feature = "minimp3", not(feature = "symphonia-mp3")),
    case("mp3", Duration::ZERO, "minimp3")
)]
#[cfg_attr(
    all(feature = "hound", not(feature = "symphonia-wav")),
    case("wav", Duration::from_secs_f64(10.143469387), "hound")
)]
#[cfg_attr(
    all(feature = "claxon", not(feature = "symphonia-flac")),
    case("flac", Duration::from_secs_f64(10.152380952), "claxon")
)]
#[cfg_attr(
    feature = "symphonia-mp3",
    case("mp3", Duration::from_secs_f64(10.187755102), "symphonia mp3")
)]
#[cfg_attr(
    feature = "symphonia-isomp4",
    case("m4a", Duration::from_secs_f64(10.188662131), "symphonia m4a")
)]
#[cfg_attr(
    feature = "symphonia-wav",
    case("wav", Duration::from_secs_f64(10.143469387), "symphonia wav")
)]
#[cfg_attr(
    feature = "symphonia-flac",
    case("flac", Duration::from_secs_f64(10.152380952), "symphonia flac")
)]
fn all_decoders(
    #[case] format: &'static str,
    #[case] correct_duration: Duration,
    #[case] decoder_name: &'static str,
) {
}

fn get_music(format: &str) -> Decoder<impl Read + Seek> {
    let asset = Path::new("assets/music").with_extension(format);
    let file = std::fs::File::open(asset).unwrap();
    let len = file.metadata().unwrap().len();
    rodio::Decoder::builder()
        .with_data(file)
        .with_byte_len(len)
        .with_seekable(true)
        .with_gapless(false)
        .build()
        .unwrap()
}

#[cfg(any(
    feature = "claxon",
    feature = "minimp3",
    feature = "symphonia-flac",
    feature = "symphonia-mp3",
    feature = "symphonia-isomp4",
    feature = "symphonia-ogg",
    feature = "symphonia-wav",
    feature = "hound",
))]
#[apply(all_decoders)]
#[trace]
fn decoder_returns_total_duration(
    #[case] format: &'static str,
    #[case] correct_duration: Duration,
    #[case] decoder_name: &'static str,
) {
    eprintln!("decoder: {decoder_name}");
    let decoder = get_music(format);
    let res = decoder
        .total_duration()
        .unwrap_or_else(|| panic!("did not return a total duration, decoder: {decoder_name}"))
        .as_secs_f64();
    let correct_duration = correct_duration.as_secs_f64();
    let abs_diff = (res - correct_duration).abs();
    assert!(
        abs_diff < 0.0001,
        "decoder got {res}, correct is: {correct_duration}"
    );
}

#[test]
fn infinite_sources_return_duration_max() {
    let sine = SineWave::new(440.0);
    assert_eq!(sine.total_duration(), Some(Duration::MAX));
}

#[test]
fn take_duration_on_infinite_source() {
    let sine = SineWave::new(440.0);
    let take = sine.take_duration(Duration::from_secs(5));
    assert_eq!(take.total_duration(), Some(Duration::from_secs(5)));
}

#[test]
fn repeat_returns_duration_max() {
    let buf = rodio::buffer::SamplesBuffer::new(
        NonZero::new(1).unwrap(),
        NonZero::new(44100).unwrap(),
        vec![0.0f32; 44100],
    );
    let repeated = buf.repeat_infinite();
    assert_eq!(repeated.total_duration(), Some(Duration::MAX));
}
