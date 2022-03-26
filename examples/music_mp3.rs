use std::io::BufReader;

fn main() {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();

    let file = std::fs::File::open("assets/music.mp3").unwrap();
    sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());

    sink.sleep_until_end();
}
