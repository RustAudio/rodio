use std::io::BufReader;
use std::time::Duration;

// hound wav decoder does not support seeking
#[cfg(feature = "hound")]
#[test]
fn seek_not_supported_returns_err() {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();

    let file = std::fs::File::open("assets/music.wav").unwrap();
    sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());
    let res = sink.try_seek(Duration::from_secs(5));
    assert_eq!(res, Err(rodio::source::SeekNotSupported { source: "test" }));
}

// mp3 decoder does support seeking
#[cfg(feature = "mp3")]
#[test]
fn seek_supported_returns_ok() {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();

    let file = std::fs::File::open("assets/music.mp3").unwrap();
    sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());
    let res = sink.try_seek(Duration::from_secs(5));
    assert_eq!(res, Ok(()));
}
