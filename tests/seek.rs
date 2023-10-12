use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;
use std::sync::Once;
use std::time::{Duration, Instant};

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};

static mut STREAM: Option<OutputStream> = None;
static mut STREAM_HANDLE: Option<OutputStreamHandle> = None;
static INIT: Once = Once::new();

fn global_stream_handle() -> &'static OutputStreamHandle {
    // mutable global access is guarded by Once therefore
    // can only happen Once and will not race
    unsafe {
        INIT.call_once(|| {
            let (stream, handle) = rodio::OutputStream::try_default().unwrap();
            STREAM = Some(stream);
            STREAM_HANDLE = Some(handle);
        });
        STREAM_HANDLE.as_ref().unwrap()
    }
}

fn time_remaining(decoder: Decoder<impl Read + Seek>) -> Duration {
    let rate = decoder.sample_rate() as f64;
    let n_channels = decoder.channels() as f64;
    let n_samples = decoder.into_iter().count() as f64;
    Duration::from_secs_f64(n_samples / rate / n_channels)
}

fn sink_and_decoder(format: &str) -> (Sink, Decoder<impl Read + Seek>) {
    let sink = rodio::Sink::try_new(global_stream_handle()).unwrap();
    let asset = Path::new("assets/music").with_extension(format);
    let file = std::fs::File::open(asset).unwrap();
    let decoder = rodio::Decoder::new(BufReader::new(file)).unwrap();
    (sink, decoder)
}

// run the following to get the other configuration:
// cargo test --no-default-features
// --features symphonia-wav --features symphonia-vorbis
// --features symphonia-flac --features symphonia-isomp4 --features minimp3
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
        let (sink, decoder) = sink_and_decoder(format);
        sink.append(decoder);
        let res = sink.try_seek(Duration::from_millis(2500));
        assert_eq!(res.is_ok(), supported, "decoder: {decoder_name}");
    }
}

// #[ignore]
#[test] // in the future use PR #510 (playback position) to speed this up
fn seek_beyond_end_saturates() {
    for (format, _, decoder_name) in format_decoder_info()
        .iter()
        .cloned()
        .filter(|(_, supported, _)| *supported)
    {
        let (sink, decoder) = sink_and_decoder(format);
        sink.append(decoder);

        println!("seeking beyond end for: {format}\t decoded by: {decoder_name}");
        let res = sink.try_seek(Duration::from_secs(999));
        assert!(res.is_ok());

        let now = Instant::now();
        sink.sleep_until_end();
        let elapsed = now.elapsed();
        assert!(elapsed.as_secs() < 1);
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

        let (_, decoder) = sink_and_decoder(format);
        let total_duration = time_remaining(decoder);

        const SEEK_BEFORE_END: Duration = Duration::from_secs(5);
        let (_, mut decoder) = sink_and_decoder(format);
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
