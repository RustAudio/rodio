use std::io::BufReader;

fn main() {
    let stream = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::new(&stream);

    let file = std::fs::File::open("examples/music.wav").unwrap();
    sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());

    sink.sleep_until_end();
}
