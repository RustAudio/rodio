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
        #[cfg(feature = "minimp3")]
        ("mp3", false, "minimp3"),
        #[cfg(feature = "symphonia-mp3")]
        ("mp3", true, "symphonia"),
        #[cfg(feature = "hound")]
        ("wav", true, "hound"),
        #[cfg(feature = "symphonia-wav")]
        ("wav", true, "symphonia"),
        #[cfg(feature = "lewton")]
        ("ogg", true, "lewton"),
        // note: disabled, symphonia returns error unsupported format
        // #[cfg(feature = "symphonia-vorbis")]
        // ("ogg", true, "symphonia"),
        #[cfg(feature = "claxon")]
        ("flac", false, "claxon"),
        #[cfg(feature = "symphonia-flac")]
        ("flac", true, "symphonia"),
        // note: disabled, symphonia returns error unsupported format
        // #[cfg(feature = "symphonia-isomp4")]
        // ("m4a", true, "symphonia"),
    ]
}

#[test]
fn seek_returns_err_if_unsupported() {
    for (format, supported, decoder) in format_decoder_info().iter().cloned() {
        println!("trying: {format},\t\tby: {decoder},\t\tshould support seek: {supported}");
        let (sink, decoder) = sink_and_decoder(format);
        sink.append(decoder);
        let res = sink.try_seek(Duration::from_secs(2));
        assert_eq!(res.is_ok(), supported);
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

fn total_duration(format: &'static str) -> Duration {
    let (sink, decoder) = sink_and_decoder(format);
    match decoder.total_duration() {
        Some(d) => d,
        None => {
            let now = Instant::now();
            sink.append(decoder);
            sink.sleep_until_end();
            now.elapsed()
        }
    }
}

#[ignore]
#[test] // in the future use PR #510 (playback position) to speed this up
fn seek_results_in_correct_remaining_playtime() {
    for (format, _, decoder_name) in format_decoder_info()
        .iter()
        .cloned()
        .filter(|(_, supported, _)| *supported)
    {
        println!("checking seek duration for: {format}\t decoded by: {decoder_name}");

        let (sink, decoder) = sink_and_decoder(format);
        sink.append(decoder);

        const SEEK_BEFORE_END: Duration = Duration::from_secs(5);
        sink.try_seek(total_duration(format) - SEEK_BEFORE_END)
            .unwrap();

        let now = Instant::now();
        sink.sleep_until_end();
        let elapsed = now.elapsed();
        let expected = SEEK_BEFORE_END;

        if elapsed.as_millis().abs_diff(expected.as_millis()) > 250 {
            panic!(
                "Seek did not result in expected leftover playtime
    leftover time: {elapsed:?}
    expected time left in source: {SEEK_BEFORE_END:?}"
            );
        }
    }
}
