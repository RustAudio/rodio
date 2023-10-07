use std::io::{BufReader, Read, Seek};
use std::path::Path;
use std::sync::Once;
use std::time::Duration;

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

fn format_decoder_info() -> &'static [(&'static str, bool, &'static str)] {
    &[
        #[cfg(feature = "minimp3")]
        ("mp3", true, "minimp3"),
        #[cfg(feature = "symphonia-mp3")]
        ("mp3", true, "symphonia"),
        #[cfg(feature = "hound")]
        ("wav", true, "hound"),
        #[cfg(feature = "symphonia-wav")]
        ("wav", true, "symphonia"),
        #[cfg(feature = "lewton")]
        ("ogg", true, "lewton"),
        #[cfg(feature = "symphonia-vorbis")]
        ("ogg", true, "symphonia"),
        #[cfg(feature = "claxon")]
        ("flac", false, "claxon"),
        #[cfg(feature = "symphonia-flac")]
        ("flac", true, "symphonia"),
        #[cfg(feature = "symphonia-isomp4")]
        ("m4a", true, "_"),
    ]
}

#[test]
fn seek_returns_err_if_unsupported() {
    for (format, supported, decoder) in format_decoder_info().iter().cloned() {
        println!("trying: {format},\t\tby: {decoder},\t\tshould support seek: {supported}");
        let (sink, decoder) = sink_and_decoder(format);
        assert_eq!(decoder.can_seek(), supported);
        sink.append(decoder);
        let res = sink.try_seek(Duration::from_secs(2));
        assert_eq!(res.is_ok(), supported);
    }
}

#[test]
fn seek_beyond_end_does_not_crash() {
    for (format, _, decoder_name) in format_decoder_info().iter().cloned() {
        let (sink, decoder) = sink_and_decoder(format);
        if !decoder.can_seek() {
            continue;
        }
        println!("seeking beyond end for: {format}\t decoded by: {decoder_name}");
        sink.append(decoder);
        sink.try_seek(Duration::from_secs(999)).unwrap();
    }
}
