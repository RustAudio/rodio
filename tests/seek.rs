use std::io::BufReader;
use std::path::Path;
use std::time::Duration;

use rodio::source::SeekNotSupported;

// hound wav decoder does not support seeking
#[cfg(feature = "hound")]
#[test]
fn seek_not_supported_returns_err() {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();

    let file = std::fs::File::open("assets/music.wav").unwrap();
    sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());
    let res = sink.try_seek(Duration::from_secs(5));

    let Err(rodio::source::SeekNotSupported { source }) = res else {
        panic!("result of try_seek should be error SourceNotSupported")
    };

    assert!(source.starts_with("rodio::decoder::wav::WavDecoder"));
}

fn play_and_seek(asset_path: &Path) -> Result<(), SeekNotSupported> {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();

    let file = std::fs::File::open(asset_path).unwrap();
    sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());
    sink.try_seek(Duration::from_secs(2))
}

#[test]
fn seek_returns_err_if_unsupported() {
    let formats = ["mp3", "wav", "ogg", "flac", "m4a"].into_iter();
    #[cfg(not(feature = "symphonia"))]
    let supported = [true, false, false, false, false].into_iter();
    #[cfg(feature = "symphonia")]
    let supported = [true, true, true, true, true].into_iter();
    #[cfg(not(feature = "symphonia"))]
    let decoder = ["minimp3", "hound", "lewton", "claxon", "_"].into_iter();
    #[cfg(feature = "symphonia")]
    let decoder = ["symphonia"].into_iter().cycle();

    for ((format, supported), decoder) in formats.zip(supported).zip(decoder) {
        println!("trying: {format} by {decoder}, should support seek: {supported}");
        let asset = Path::new("assets/music").with_extension(format);
        let res = play_and_seek(&asset);
        assert_eq!(res.is_ok(), supported);
    }
}
