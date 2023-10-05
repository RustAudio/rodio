use std::io::BufReader;
use std::path::Path;
use std::time::Duration;

use rodio::source::SeekNotSupported;

fn play_and_seek(asset_path: &Path) -> Result<(), SeekNotSupported> {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();

    let file = std::fs::File::open(asset_path).unwrap();
    sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());
    sink.try_seek(Duration::from_secs(2))
}

#[test]
fn seek_returns_err_if_unsupported() {
    let formats = [
        #[cfg(feature = "minimp3")]
        ("mp3", true, "minimp3"),
        #[cfg(feature = "symphonia-mp3")]
        ("mp3", true, "symphonia"),
        #[cfg(feature = "hound")]
        ("wav", false, "hound"),
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
    ];

    for (format, supported, decoder) in formats {
        println!("trying: {format} by {decoder}, should support seek: {supported}");
        let asset = Path::new("assets/music").with_extension(format);
        let res = play_and_seek(&asset);
        assert_eq!(res.is_ok(), supported);
    }
}
