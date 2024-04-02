use std::io::{BufReader, Read, Seek};
use std::path::Path;
use std::time::Duration;

use rodio::{buffer, Decoder, Source};

fn time_remaining(decoder: Decoder<impl Read + Seek>) -> Duration {
    let rate = decoder.sample_rate() as f64;
    let n_channels = decoder.channels() as f64;
    let n_samples = decoder.into_iter().count() as f64;
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

#[test]
fn seek_does_not_break_channel_order() {
    let file = std::fs::File::open("assets/RL.ogg").unwrap();
    let mut source = rodio::Decoder::new(BufReader::new(file)).unwrap();
    assert_eq!(source.channels(), 2, "test needs a stereo beep file");
    let channels = source.channels() as usize;
    let sample_rate = source.sample_rate() as usize;
    let samples: Vec<f32> = source.convert_samples().collect();

    const MARGIN: usize = 30;
    let beep_starts = samples
        .iter()
        .enumerate()
        .find(|(_, s)| s.abs() > 0.001)
        .expect("track should not be silent")
        .0
        + MARGIN;

    const BASICALLY_ZERO: f32 = 0.00001;
    let beep_ends = samples
        .chunks_exact(8)
        .enumerate()
        .map(|(idx, chunk)| (idx * 8, chunk))
        .skip(beep_starts / 8)
        .find(|(_, s)| s.iter().skip(1).step_by(2).all(|s| s.abs() < BASICALLY_ZERO))
        .expect("beep should end")
        .0
        - MARGIN;

    dbg!(beep_starts, beep_ends);
    let beep_duration = (beep_ends - beep_starts) as f32 / sample_rate as f32;
    let beep_duration = Duration::from_secs_f32(beep_duration);
    let beep = &samples[beep_starts..beep_ends];

    dbg!(beep_duration);
    // let left_channel = samples.iter().step_by(channels);
    // let left_volume = left_channel.map(|s| s.abs()).sum::<f32>() / (beep_len / channels) as f32;
    //
    // let right_channel = samples.iter().skip(1).step_by(channels);
    // let right_volume = right_channel.map(|s| s.abs()).sum::<f32>() / (beep_len / channels) as f32;

    // assert_eq!(right_volume, 0.0);
    // assert_eq!(left_volume, 0.0);
    // assert!(
    //     right_volume ==
    //     "The left channel not silent while the right is is needed for the test"
    // );
}
