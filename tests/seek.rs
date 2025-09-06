#![allow(dead_code)]
#![allow(unused_imports)]

use std::{
    io::{Read, Seek},
    path::Path,
    time::Duration,
};

#[cfg(any(
    feature = "symphonia-mp3",
    all(feature = "symphonia-ogg", feature = "symphonia-vorbis")
))]
use rodio::source::SeekError;
use rodio::{ChannelCount, Decoder, Float, Sample, Source};

use rstest::rstest;
use rstest_reuse::{self, *};

// Test constants
const BASICALLY_ZERO: Sample = 0.0001;
const ONE_SECOND: Duration = Duration::from_secs(1);

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
#[cfg_attr(feature = "claxon", case("flac", "claxon"))]
#[cfg_attr(feature = "hound", case("wav", "hound"))]
#[cfg_attr(feature = "lewton", case("ogg", "lewton"))]
#[cfg_attr(feature = "minimp3", case("mp3", "minimp3"))]
#[cfg_attr(
    all(feature = "symphonia-flac", not(feature = "claxon")),
    case("flac", "symphonia")
)]
#[cfg_attr(
    all(feature = "symphonia-isomp4", feature = "symphonia-aac"),
    case("m4a", "symphonia")
)]
#[cfg_attr(
    all(feature = "symphonia-mp3", not(feature = "minimp3")),
    case("mp3", "symphonia")
)]
#[cfg_attr(
    all(
        feature = "symphonia-ogg",
        feature = "symphonia-vorbis",
        not(feature = "lewton")
    ),
    case("ogg", "symphonia")
)]
#[cfg_attr(
    all(
        feature = "symphonia-pcm",
        feature = "symphonia-wav",
        not(feature = "hound")
    ),
    case("wav", "symphonia")
)]
fn all_decoders(#[case] format: &'static str, #[case] decoder_name: &'static str) {}

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
fn seek_beyond_end_saturates(#[case] format: &'static str, #[case] decoder_name: &'static str) {
    println!("seeking beyond end for: {format}\t decoded by: {decoder_name}");

    let mut decoder = get_music(format);
    let res = decoder.try_seek(Duration::from_secs(999));

    assert!(res.is_ok(), "err: {res:?}");
    assert!(time_remaining(decoder) < ONE_SECOND);
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
fn seek_possible_after_exhausting_source(
    #[case] format: &'static str,
    #[case] decoder_name: &'static str,
) {
    println!("checking seek possibility after exhausting source for: {format}\t decoded by: {decoder_name}");

    let mut source = get_music(format);
    while source.next().is_some() {}
    assert!(source.next().is_none());

    source.try_seek(Duration::ZERO).unwrap();
    assert!(source.next().is_some());
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
fn seek_does_not_break_channel_order(
    #[case] format: &'static str,
    #[case] decoder_name: &'static str,
) {
    match (format, decoder_name) {
        ("m4a", "symphonia") => {
            // skip this test for m4a while the symphonia decoder has issues with aac timing.
            // TODO: re-investigate when symphonia 0.5.5 or greater is released.
            return;
        }
        ("mp3", "minimp3") => {
            // skip this test for mp3 because seeking is coarse and does not work well with
            // the stereo beep file which is variable bitrate.
            return;
        }
        _ => {}
    }

    println!("checking channel order after seek for: {format}\t decoded by: {decoder_name}");

    let mut source = get_rl(format);
    let channels = source.channels();
    assert_eq!(channels.get(), 2, "test needs a stereo beep file");

    let beep_range = second_channel_beep_range(&mut source);
    let beep_start = Duration::from_secs_f32(
        beep_range.start as f32
            / source.channels().get() as f32
            / source.sample_rate().get() as f32,
    );

    let mut source = get_rl(format);

    let mut channel_offset = 0;
    for offset in [1, 4, 7, 40, 41, 120, 179]
        .map(|offset| offset as f32 / (source.sample_rate().get() as f32))
        .map(Duration::from_secs_f32)
    {
        source.next(); // WINDOW is even, make the amount of calls to next
                       // uneven to force issues with channels alternating
                       // between seek to surface
        channel_offset = (channel_offset + 1) % 2;

        source.try_seek(beep_start + offset).unwrap();
        let samples: Vec<_> = source.by_ref().take(100).collect();
        let channel0 = channel_offset;
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

#[rstest]
#[cfg(any(
    feature = "symphonia-mp3",
    all(feature = "symphonia-ogg", feature = "symphonia-vorbis",)
))]
#[cfg_attr(all(feature = "symphonia-mp3"), case("mp3", "symphonia"))]
#[cfg_attr(
    all(feature = "symphonia-ogg", feature = "symphonia-vorbis",),
    case("ogg", "symphonia")
)]
fn random_access_seeks(#[case] format: &'static str, #[case] decoder_name: &'static str) {
    println!("checking random access seeks for: {format}\t decoded by: {decoder_name}");

    // Decoder::new::<File> does *not* set byte_len or is_seekable
    let file = std::fs::File::open(Path::new("assets/music").with_extension(format)).unwrap();
    let mut decoder = Decoder::new(file).unwrap();
    assert!(
        decoder.try_seek(Duration::from_secs(2)).is_ok(),
        "forward seek should work without byte_len"
    );
    assert!(
        matches!(decoder.try_seek(ONE_SECOND), Err(SeekError::ForwardOnly)),
        "backward seek should fail without byte_len"
    );

    // Decoder::try_from::<File> sets byte_len and is_seekable
    let mut decoder = get_music(format);
    assert!(
        decoder.try_seek(ONE_SECOND * 2).is_ok(),
        "forward seek should work with byte_len and is_seekable"
    );
    assert!(
        decoder.try_seek(ONE_SECOND).is_ok(),
        "backward seek should work with byte_len and is_seekable"
    );
}

fn second_channel_beep_range<R: rodio::Source>(source: &mut R) -> std::ops::Range<usize> {
    let channels = source.channels().get() as usize;
    let samples: Vec<Sample> = source.by_ref().collect();

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
                .sum::<Sample>()
                > 0.1
        })
        .expect("track should not be silent")
        .0
        .next_multiple_of(channels);

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
    assert!(
        is_silent(samples, ChannelCount::new(channels as u16).unwrap(), 0),
        "{samples:?}"
    );
    assert!(
        !is_silent(samples, ChannelCount::new(channels as u16).unwrap(), 1),
        "{samples:?}"
    );

    beep_starts..beep_ends
}

fn is_silent(samples: &[Sample], channels: ChannelCount, channel: usize) -> bool {
    assert_eq!(samples.len(), 100);
    let channel = samples
        .iter()
        .skip(channel)
        .step_by(channels.get() as usize);
    let volume = channel.map(|s| s.abs()).sum::<Sample>() / samples.len() as Sample
        * channels.get() as Sample;

    volume < BASICALLY_ZERO
}

fn time_remaining(decoder: Decoder<impl Read + Seek>) -> Duration {
    let rate = decoder.sample_rate().get() as f64;
    let n_channels = decoder.channels().get() as f64;
    let n_samples = decoder.into_iter().count() as f64;
    Duration::from_secs_f64(n_samples / rate / n_channels)
}

fn get_music(format: &str) -> Decoder<impl Read + Seek> {
    let asset = Path::new("assets/music").with_extension(format);
    Decoder::try_from(asset).unwrap()
}

fn get_rl(format: &str) -> Decoder<impl Read + Seek> {
    let asset = Path::new("assets/RL").with_extension(format);
    Decoder::try_from(asset).unwrap()
}
