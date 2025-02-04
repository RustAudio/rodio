use std::io::{BufReader, Read, Seek};
use std::path::Path;
use std::time::Duration;

use rodio::{Decoder, Source};

use rstest::rstest;
use rstest_reuse::{self, *};

#[template]
#[rstest]
#[cfg_attr(
    feature = "symphonia-vorbis",
    case("ogg", Duration::from_secs_f64(0.0), "symphonia")
)]
#[cfg_attr(
    all(feature = "minimp3", not(feature = "symphonia-mp3")),
    case("mp3", Duration::ZERO, "minimp3")
)]
#[cfg_attr(
    all(feature = "wav", not(feature = "symphonia-wav")),
    case("wav", Duration::from_secs_f64(20.286938775), "hound")
)]
#[cfg_attr(
    all(feature = "flac", not(feature = "symphonia-flac")),
    case("flac", Duration::from_secs_f64(10.152380952), "claxon")
)]
#[cfg_attr(
    feature = "symphonia-mp3",
    case("mp3", Duration::from_secs_f64(10.000000), "symphonia mp3")
)]
// note: disabled, broken decoder see issue: #577
#[cfg_attr(
    feature = "symphonia-isomp4",
    case("m4a", Duration::from_secs_f64(10.000000), "symphonia m4a")
)]
#[cfg_attr(
    feature = "symphonia-wav",
    case("wav", Duration::from_secs_f64(10.000000), "symphonia wav")
)]
#[cfg_attr(
    feature = "symphonia-flac",
    case("flac", Duration::from_secs_f64(10.00000), "symphonia flac")
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
    rodio::Decoder::new(BufReader::new(file)).unwrap()
}

#[apply(all_decoders)]
#[trace]
fn seek_returns_err_if_unsupported(
    #[case] format: &'static str,
    #[case] correct_duration: Duration,
    #[case] decoder_name: &'static str,
) {
    eprintln!("decoder: {decoder_name}");
    let decoder = get_music(format);
    let res = decoder
        .total_duration()
        .expect(&format!("did not return a total duration, decoder: {decoder_name}"))
        .as_secs_f64();
    let correct_duration = correct_duration.as_secs_f64();
    let abs_diff = (res - correct_duration).abs();
    assert!(
        abs_diff < 0.0001,
        "decoder got {res}, correct is: {correct_duration}"
    );
}
