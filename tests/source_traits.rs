#![allow(dead_code)]
#![allow(unused_imports)]

use std::io::{Read, Seek};
use std::num::NonZeroU32;
use std::path::Path;
use std::time::Duration;

use rodio::{Decoder, Source};

use rstest::rstest;
use rstest_reuse::{self, *};

#[cfg(any(
    feature = "claxon",
    feature = "hound",
    feature = "lewton",
    feature = "minimp3",
    all(feature = "symphonia-aac", feature = "symphonia-isomp4"),
    feature = "symphonia-flac",
    feature = "symphonia-mp3",
    all(feature = "symphonia-ogg", feature = "symphonia-vorbis"),
    all(feature = "symphonia-pcm", feature = "symphonia-wav"),
))]
#[template]
#[rstest]
#[cfg_attr(
    feature = "claxon",
    case("flac", Duration::from_secs_f64(10.152380952), "claxon")
)]
#[cfg_attr(
    feature = "hound",
    case("wav", Duration::from_secs_f64(10.143469387), "hound")
)]
#[cfg_attr(
    feature = "lewton",
    case("ogg", Duration::from_secs_f64(69.328979591), "lewton")
)]
#[cfg_attr(
    feature = "minimp3",
    case("mp3", Duration::from_secs_f64(10.187755102), "minimp3")
)]
#[cfg_attr(
    all(feature = "symphonia-aac", feature = "symphonia-isomp4"),
    case("m4a", Duration::from_secs_f64(10.188662131), "symphonia m4a")
)]
#[cfg_attr(
    all(feature = "symphonia-flac", not(feature = "claxon")),
    case("flac", Duration::from_secs_f64(10.152380952), "symphonia flac")
)]
#[cfg_attr(
    all(feature = "symphonia-mp3", not(feature = "minimp3")),
    case("mp3", Duration::from_secs_f64(10.187755102), "symphonia mp3")
)]
#[cfg_attr(
    all(
        feature = "symphonia-ogg",
        feature = "symphonia-vorbis",
        not(feature = "lewton")
    ),
    case("ogg", Duration::from_secs_f64(69.328979591), "symphonia")
)]
#[cfg_attr(
    all(
        feature = "symphonia-pcm",
        feature = "symphonia-wav",
        not(feature = "hound")
    ),
    case("wav", Duration::from_secs_f64(10.143469387), "symphonia wav")
)]
fn all_decoders(
    #[case] format: &'static str,
    #[case] correct_duration: Duration,
    #[case] decoder_name: &'static str,
) {
}

#[cfg(any(
    feature = "lewton",
    feature = "minimp3",
    all(feature = "symphonia-aac", feature = "symphonia-isomp4"),
    feature = "symphonia-mp3",
    all(feature = "symphonia-ogg", feature = "symphonia-vorbis"),
))]
#[template]
#[rstest]
#[cfg_attr(feature = "lewton", case("ogg", "lewton"))]
#[cfg_attr(feature = "minimp3", case("mp3", "minimp3"))]
#[cfg_attr(
    all(feature = "symphonia-aac", feature = "symphonia-isomp4"),
    case("m4a", "symphonia m4a")
)]
#[cfg_attr(
    all(feature = "symphonia-mp3", not(feature = "minimp3")),
    case("mp3", "symphonia mp3")
)]
#[cfg_attr(
    all(
        feature = "symphonia-ogg",
        feature = "symphonia-vorbis",
        not(feature = "lewton")
    ),
    case("ogg", "symphonia")
)]
fn decoders_with_variable_spans(#[case] format: &'static str, #[case] decoder_name: &'static str) {}

#[cfg(any(
    feature = "lewton",
    feature = "minimp3",
    all(feature = "symphonia-aac", feature = "symphonia-isomp4"),
    feature = "symphonia-mp3",
    all(feature = "symphonia-ogg", feature = "symphonia-vorbis"),
))]
#[template]
#[rstest]
#[cfg_attr(all(feature = "lewton",), case("ogg", "lewton"))]
#[cfg_attr(all(feature = "minimp3"), case("mp3", "minimp3"))]
#[cfg_attr(
    all(feature = "symphonia-isomp4", feature = "symphonia-aac"),
    case("m4a", "symphonia m4a")
)]
#[cfg_attr(
    all(feature = "symphonia-mp3", not(feature = "minimp3")),
    case("mp3", "symphonia mp3")
)]
#[cfg_attr(
    all(
        feature = "symphonia-ogg",
        feature = "symphonia-vorbis",
        not(feature = "lewton")
    ),
    case("ogg", "symphonia")
)]
fn lossy_decoders(#[case] format: &'static str, #[case] decoder_name: &'static str) {}

#[cfg(any(
    feature = "claxon",
    feature = "hound",
    feature = "symphonia-flac",
    all(feature = "symphonia-pcm", feature = "symphonia-wav"),
))]
#[template]
#[rstest]
#[cfg_attr(feature = "claxon", case("flac", 16, "claxon"))]
#[cfg_attr(feature = "hound", case("wav", 16, "hound"))]
#[cfg_attr(
    all(feature = "symphonia-flac", not(feature = "claxon")),
    case("flac", 16, "symphonia flac")
)]
#[cfg_attr(
    all(
        feature = "symphonia-pcm",
        feature = "symphonia-wav",
        not(feature = "hound")
    ),
    case("wav", 16, "symphonia wav")
)]
fn lossless_decoders(
    #[case] format: &'static str,
    #[case] bit_depth: u32,
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
        .with_scan_duration(true)
        .with_gapless(false)
        .build()
        .unwrap()
}

#[cfg(any(
    feature = "claxon",
    feature = "hound",
    feature = "lewton",
    feature = "minimp3",
    all(feature = "symphonia-aac", feature = "symphonia-isomp4"),
    feature = "symphonia-flac",
    feature = "symphonia-mp3",
    all(feature = "symphonia-ogg", feature = "symphonia-vorbis"),
    all(feature = "symphonia-pcm", feature = "symphonia-wav"),
))]
#[apply(all_decoders)]
#[trace]
fn decoder_returns_total_duration(
    #[case] format: &'static str,
    #[case] correct_duration: Duration,
    #[case] decoder_name: &'static str,
) {
    println!("decoder: {decoder_name}");
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

#[cfg(any(
    feature = "lewton",
    feature = "minimp3",
    all(feature = "symphonia-aac", feature = "symphonia-isomp4"),
    feature = "symphonia-mp3",
    all(feature = "symphonia-ogg", feature = "symphonia-vorbis"),
))]
#[apply(decoders_with_variable_spans)]
#[trace]
fn decoder_returns_non_zero_span_length(
    #[case] format: &'static str,
    #[case] decoder_name: &'static str,
) {
    println!("decoder: {decoder_name}");
    let decoder = get_music(format);
    let span_len = decoder
        .current_span_len()
        .expect("decoder should return Some(len) for variable parameter formats");

    assert!(
        span_len > 0,
        "decoder {decoder_name} returned Some(0) span length, which indicates a buffering problem with test assets"
    );
}

#[cfg(any(
    feature = "claxon",
    feature = "hound",
    feature = "lewton",
    feature = "minimp3",
    all(feature = "symphonia-aac", feature = "symphonia-isomp4"),
    feature = "symphonia-flac",
    feature = "symphonia-mp3",
    all(feature = "symphonia-ogg", feature = "symphonia-vorbis"),
    all(feature = "symphonia-pcm", feature = "symphonia-wav"),
))]
#[apply(all_decoders)]
#[trace]
fn decoder_returns_correct_channels(
    #[case] format: &'static str,
    #[case] _correct_duration: Duration,
    #[case] decoder_name: &'static str,
) {
    use std::num::NonZero;

    use rodio::ChannelCount;

    println!("decoder: {decoder_name}");
    let decoder = get_music(format);
    let channels = decoder.channels();

    // All our test files should be stereo (2 channels)
    assert_eq!(
        channels,
        ChannelCount::new(2).unwrap(),
        "decoder {decoder_name} returned {channels} channels, expected 2 (stereo)"
    );
}

#[cfg(any(
    feature = "lewton",
    feature = "minimp3",
    all(feature = "symphonia-aac", feature = "symphonia-isomp4"),
    feature = "symphonia-mp3",
    all(feature = "symphonia-ogg", feature = "symphonia-vorbis"),
))]
#[apply(lossy_decoders)]
#[trace]
fn decoder_returns_none_bit_depth(
    #[case] format: &'static str,
    #[case] decoder_name: &'static str,
) {
    println!("decoder: {decoder_name}");
    let decoder = get_music(format);
    let bit_depth = decoder.bits_per_sample();

    assert!(
        bit_depth.is_none(),
        "decoder {decoder_name} returned Some({:?}) bit depth, expected None for lossy formats",
        bit_depth.unwrap()
    );
}

#[cfg(any(
    feature = "claxon",
    feature = "hound",
    feature = "symphonia-flac",
    all(feature = "symphonia-pcm", feature = "symphonia-wav"),
))]
#[apply(lossless_decoders)]
#[trace]
fn decoder_returns_some_bit_depth(
    #[case] format: &'static str,
    #[case] bit_depth: u32,
    #[case] decoder_name: &'static str,
) {
    println!("decoder: {decoder_name}");
    let decoder = get_music(format);
    let returned_bit_depth = decoder.bits_per_sample();
    assert_eq!(
        returned_bit_depth,
        NonZeroU32::new(bit_depth),
        "decoder {decoder_name} returned {returned_bit_depth:?} bit depth, expected {bit_depth}"
    );
}
#[cfg(any(
    feature = "claxon",
    feature = "hound",
    feature = "symphonia-flac",
    all(feature = "symphonia-pcm", feature = "symphonia-wav")
))]
#[test]
fn decoder_returns_hi_res_bit_depths() {
    const CASES: [(&str, u32); 3] = [
        ("audacity24bit_level5.flac", 24),
        ("audacity32bit.wav", 32),
        ("audacity32bit_int.wav", 32),
    ];

    for (asset, bit_depth) in CASES {
        let file = std::fs::File::open(format!("assets/{asset}")).unwrap();
        if let Ok(decoder) = rodio::Decoder::try_from(file) {
            // TODO: Symphonia returns None for audacity32bit.wav (float)
            if let Some(returned_bit_depth) = decoder.bits_per_sample() {
                assert_eq!(
                    returned_bit_depth.get(),
                    bit_depth,
                    "decoder for {asset} returned {returned_bit_depth:?} bit depth, expected {bit_depth}"
                );
            }
        }
    }
}
