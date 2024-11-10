use std::io::{BufReader, Read, Seek};
use std::path::Path;
use std::time::Duration;

use rodio::{Decoder, Source};
use rstest::rstest;
use rstest_reuse::{self, *};

#[template]
#[rstest]
// note: disabled, broken decoder see issue: #516 and #539
// #[cfg_attr(feature = "symphonia-vorbis"), case("ogg", true, "symphonia")],
#[cfg_attr(
    all(feature = "minimp3", not(feature = "symphonia-mp3")),
    case("mp3", false, "minimp3")
)]
#[cfg_attr(
    all(feature = "wav", not(feature = "symphonia-wav")),
    case("wav", true, "hound")
)]
#[cfg_attr(
    all(feature = "flac", not(feature = "symphonia-flac")),
    case("flac", false, "claxon")
)]
#[cfg_attr(feature = "symphonia-mp3", case("mp3", true, "symphonia"))]
// note: disabled, broken decoder see issue: #577
#[cfg_attr(feature = "symphonia-isomp4", case("m4a", true, "symphonia"))]
#[cfg_attr(feature = "symphonia-wav", case("wav", true, "symphonia"))]
#[cfg_attr(feature = "symphonia-flac", case("flac", true, "symphonia"))]
fn all_decoders(
    #[case] format: &'static str,
    #[case] supports_seek: bool,
    #[case] decoder_name: &'static str,
) {
}

#[template]
#[rstest]
// note: disabled, broken decoder see issue: #516 and #539
// #[cfg_attr(feature = "symphonia-vorbis"), case("ogg", true, "symphonia")],
#[cfg_attr(
    all(feature = "wav", not(feature = "symphonia-wav")),
    case("wav", "hound")
)]
#[cfg_attr(feature = "symphonia-mp3", case("mp3", "symphonia"))]
// note: disabled, broken decoder see issue: #577
// #[cfg_attr(feature = "symphonia-isomp4", case("m4a", "symphonia"))]
#[cfg_attr(feature = "symphonia-wav", case("wav", "symphonia"))]
#[cfg_attr(feature = "symphonia-flac", case("flac", "symphonia"))]
fn supported_decoders(#[case] format: &'static str, #[case] decoder_name: &'static str) {}

#[apply(all_decoders)]
#[trace]
fn seek_returns_err_if_unsupported(
    #[case] format: &'static str,
    #[case] supports_seek: bool,
    #[case] decoder_name: &'static str,
) {
    let mut decoder = get_music(format);
    let res = decoder.try_seek(Duration::from_millis(2500));
    assert_eq!(res.is_ok(), supports_seek, "decoder: {decoder_name}");
}

#[apply(supported_decoders)]
#[trace]
fn seek_beyond_end_saturates(#[case] format: &'static str, #[case] decoder_name: &'static str) {
    let mut decoder = get_music(format);
    println!("seeking beyond end for: {format}\t decoded by: {decoder_name}");
    let res = decoder.try_seek(Duration::from_secs(999));

    assert!(res.is_ok(), "err: {res:?}");
    assert!(time_remaining(decoder) < Duration::from_secs(1));
}

#[apply(supported_decoders)]
#[trace]
fn seek_results_in_correct_remaining_playtime(
    #[case] format: &'static str,
    #[case] decoder_name: &'static str,
) {
    println!("checking seek duration for: {format}\t decoded by: {decoder_name}");

    let decoder = get_music(format);
    let total_duration = time_remaining(decoder);
    dbg!(total_duration);

    const SEEK_BEFORE_END: Duration = Duration::from_secs(5);
    let mut decoder = get_music(format);
    decoder.try_seek(total_duration - SEEK_BEFORE_END).unwrap();

    let after_seek = time_remaining(decoder);
    let expected = SEEK_BEFORE_END;

    if after_seek.as_millis().abs_diff(expected.as_millis()) > 250 {
        panic!(
            "Seek did not result in expected leftover playtime
    leftover time: {after_seek:?}
    expected time left in source: {SEEK_BEFORE_END:?}"
        );
    }
}

#[apply(supported_decoders)]
#[trace]
fn seek_possible_after_exausting_source(
    #[case] format: &'static str,
    #[case] _decoder_name: &'static str,
) {
    let mut source = get_music(format);
    while source.next().is_some() {}
    assert!(source.next().is_none());

    source.try_seek(Duration::from_secs(0)).unwrap();
    assert!(source.next().is_some());
}

#[apply(supported_decoders)]
#[trace]
fn seek_does_not_break_channel_order(
    #[case] format: &'static str,
    #[case] _decoder_name: &'static str,
) {
    let mut source = get_rl(format).convert_samples();
    let channels = source.channels();
    assert_eq!(channels, 2, "test needs a stereo beep file");

    let beep_range = second_channel_beep_range(&mut source);
    let beep_start = Duration::from_secs_f32(
        beep_range.start as f32 / source.channels() as f32 / source.sample_rate() as f32,
    );

    let mut source = get_rl(format).convert_samples();

    let mut channel_offset = 0;
    for offset in [1, 4, 7, 40, 41, 120, 179]
        .map(|offset| offset as f32 / (source.sample_rate() as f32))
        .map(Duration::from_secs_f32)
    {
        source.next(); // WINDOW is even, make the amount of calls to next
                       // uneven to force issues with channels alternating
                       // between seek to surface
        channel_offset = (channel_offset + 1) % 2;

        source.try_seek(beep_start + offset).unwrap();
        let samples: Vec<_> = source.by_ref().take(100).collect();
        let channel0 = 0 + channel_offset;
        assert!(
            is_silent(&samples, source.channels(), channel0),
            "channel0 should be silent, 
    channel0 starts at idx: {channel0}
    seek: {beep_start:?} + {offset:?}
    samples: {samples:?}"
        );
        let channel1 = (1 + channel_offset) % 2;
        assert!(
            !is_silent(&samples, source.channels(), channel1),
            "channel1 should not be silent, 
    channel1; starts at idx: {channel1}
    seek: {beep_start:?} + {offset:?}
    samples: {samples:?}"
        );
    }
}

fn second_channel_beep_range<R: rodio::Source>(source: &mut R) -> std::ops::Range<usize>
where
    R: Iterator<Item = f32>,
{
    let channels = source.channels() as usize;
    let samples: Vec<f32> = source.by_ref().collect();

    const WINDOW: usize = 50;
    let beep_starts = samples
        .chunks_exact(WINDOW)
        .enumerate()
        .map(|(idx, chunk)| (idx * WINDOW, chunk))
        .find(|(_, s)| {
            s.iter()
                .skip(1)
                .step_by(channels)
                .map(|s| s.abs())
                .sum::<f32>()
                > 0.1
        })
        .expect("track should not be silent")
        .0
        .next_multiple_of(channels);

    const BASICALLY_ZERO: f32 = 0.0001;
    let beep_ends = samples
        .chunks_exact(WINDOW)
        .enumerate()
        .map(|(idx, chunk)| (idx * WINDOW, chunk))
        .skip(beep_starts / WINDOW)
        .find(|(_, s)| {
            s.iter()
                .skip(1)
                .step_by(channels)
                .all(|s| s.abs() < BASICALLY_ZERO)
        })
        .expect("beep should end")
        .0
        .next_multiple_of(channels);

    let samples = &samples[beep_starts..beep_starts + 100];
    assert!(is_silent(samples, channels as u16, 0), "{samples:?}");
    assert!(!is_silent(samples, channels as u16, 1), "{samples:?}");

    beep_starts..beep_ends
}

fn is_silent(samples: &[f32], channels: u16, channel: usize) -> bool {
    assert_eq!(samples.len(), 100);
    let channel = samples.iter().skip(channel).step_by(channels as usize);
    let volume =
        channel.map(|s| s.abs()).sum::<f32>() as f32 / samples.len() as f32 * channels as f32;

    const BASICALLY_ZERO: f32 = 0.0001;
    volume < BASICALLY_ZERO
}

fn time_remaining(decoder: Decoder<impl Read + Seek>) -> Duration {
    let rate = decoder.sample_rate() as f64;
    let n_channels = decoder.channels() as f64;
    let n_samples = decoder.into_iter().count() as f64;
    Duration::from_secs_f64(n_samples / rate / n_channels)
}

fn get_music(format: &str) -> Decoder<impl Read + Seek> {
    let asset = Path::new("assets/music").with_extension(format);
    let file = std::fs::File::open(asset).unwrap();
    let decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
    decoder
}

fn get_rl(format: &str) -> Decoder<impl Read + Seek> {
    let asset = Path::new("assets/RL").with_extension(format);
    println!("opening: {}", asset.display());
    let file = std::fs::File::open(asset).unwrap();
    let decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
    decoder
}
