use std::io::BufReader;
use std::time::Duration;

fn main() {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();

    let file = std::fs::File::open("examples/music.mp3").unwrap();
    sink.append_seekable(rodio::Decoder::new(BufReader::new(file)).unwrap());

    std::thread::sleep(std::time::Duration::from_secs(2));
    sink.seek(Duration::from_secs(0));

    std::thread::sleep(std::time::Duration::from_secs(2));
    sink.seek(Duration::from_secs(4));

    sink.sleep_until_end();
}
