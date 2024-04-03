use std::io::{BufReader, Read, Seek};
use std::path::Path;
use std::time::Duration;

use rodio::{Decoder, Source};

fn time_remaining(decoder: Decoder<impl Read + Seek>) -> Duration {
    let rate = decoder.sample_rate() as f64;
    let n_channels = decoder.channels() as f64;
    let n_samples = decoder.into_iter().count() as f64;
    dbg!(n_samples);
    Duration::from_secs_f64(n_samples / rate / n_channels)
}

fn get_decoder(format: &str) -> Decoder<impl Read + Seek> {
    let asset = Path::new("assets/music").with_extension(format);
    let file = std::fs::File::open(asset).unwrap();
    let decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
    decoder
}

// run tests twice to test all decoders
// cargo test
// cargo test --features symphonia-all
fn format_decoder_info() -> &'static [(&'static str, bool, &'static str)] {
    &[
        #[cfg(all(feature = "minimp3", not(feature = "symphonia-mp3")))]
        ("mp3", false, "minimp3"),
        #[cfg(feature = "symphonia-mp3")]
        ("mp3", true, "symphonia"),
        #[cfg(all(feature = "wav", not(feature = "symphonia-wav")))]
        ("wav", true, "hound"),
        #[cfg(feature = "symphonia-wav")]
        ("wav", true, "symphonia"),
        #[cfg(all(feature = "vorbis", not(feature = "symphonia-vorbis")))]
        ("ogg", true, "lewton"),
        // note: disabled, broken decoder see issue: #516
        // #[cfg(feature = "symphonia-vorbis")]
        // ("ogg", true, "symphonia"),
        #[cfg(all(feature = "flac", not(feature = "symphonia-flac")))]
        ("flac", false, "claxon"),
        #[cfg(feature = "symphonia-flac")]
        ("flac", true, "symphonia"),
        // note: disabled, symphonia returns error unsupported format
        #[cfg(feature = "symphonia-isomp4")]
        ("m4a", true, "symphonia"),
    ]
}

#[test]
fn seek_returns_err_if_unsupported() {
    for (format, supported, decoder_name) in format_decoder_info().iter().cloned() {
        println!("trying: {format},\t\tby: {decoder_name},\t\tshould support seek: {supported}");
        let mut decoder = get_decoder(format);
        let res = decoder.try_seek(Duration::from_millis(2500));
        assert_eq!(res.is_ok(), supported, "decoder: {decoder_name}");
    }
}

#[test] // in the future use PR #510 (playback position) to speed this up
fn seek_beyond_end_saturates() {
    for (format, _, decoder_name) in format_decoder_info()
        .iter()
        .cloned()
        .filter(|(_, supported, _)| *supported)
    {
        let mut decoder = get_decoder(format);

        println!("seeking beyond end for: {format}\t decoded by: {decoder_name}");
        let res = decoder.try_seek(Duration::from_secs(999));
        assert!(res.is_ok());

        assert!(time_remaining(decoder) < Duration::from_secs(1));
    }
}

#[test]
fn seek_results_in_correct_remaining_playtime() {
    for (format, _, decoder_name) in format_decoder_info()
        .iter()
        .cloned()
        .filter(|(_, supported, _)| *supported)
    {
        println!("checking seek duration for: {format}\t decoded by: {decoder_name}");

        let decoder = get_decoder(format);
        let total_duration = time_remaining(decoder);
        dbg!(total_duration);

        const SEEK_BEFORE_END: Duration = Duration::from_secs(5);
        let mut decoder = get_decoder(format);
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
        .0;
    let beep_starts = beep_starts.next_multiple_of(channels);

    const BASICALLY_ZERO: f32 = 0.00001;
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
        .0;
    let beep_ends = beep_ends.next_multiple_of(channels);

    beep_starts..beep_ends
}

fn is_silent(samples: &[f32], channels: u16, channel: usize) -> bool {
    assert_eq!(samples.len(), 100);
    // dbg!(samples);
    let channel = samples.iter().skip(channel).step_by(channels as usize);
    let volume =
        channel.map(|s| s.abs()).sum::<f32>() as f32 / samples.len() as f32 * channels as f32;

    const BASICALLY_ZERO: f32 = 0.0001;
    volume < BASICALLY_ZERO
}

// TODO test all decoders
#[test]
fn seek_does_not_break_channel_order() {
    let file = std::fs::File::open("assets/RL.ogg").unwrap();
    let mut source = rodio::Decoder::new(BufReader::new(file))
        .unwrap()
        .convert_samples();
    let channels = source.channels();
    assert_eq!(channels, 2, "test needs a stereo beep file");

    let beep_range = second_channel_beep_range(&mut source);
    let beep_start = Duration::from_secs_f32(
        beep_range.start as f32 / source.channels() as f32 / source.sample_rate() as f32,
    );

    let file = std::fs::File::open("assets/RL.ogg").unwrap();
    let mut source = rodio::Decoder::new(BufReader::new(file))
        .unwrap()
        .convert_samples();

    const WINDOW: usize = 100;
    let samples: Vec<_> = source
        .by_ref()
        .skip(beep_range.start)
        .take(WINDOW)
        .collect();
    assert!(is_silent(&samples, channels, 0), "{samples:?}");
    assert!(!is_silent(&samples, channels, 1), "{samples:?}");

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
        let samples: Vec<_> = source.by_ref().take(WINDOW).collect();
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

// TODO test all decoders
#[test]
fn seek_possible_after_exausting_source() {
    let file = std::fs::File::open("assets/RL.ogg").unwrap();
    let mut source = rodio::Decoder::new(BufReader::new(file)).unwrap();
    while source.next().is_some() {}
    assert!(source.next().is_none());

    source.try_seek(Duration::from_secs(0)).unwrap();
    assert!(source.next().is_some());
}
