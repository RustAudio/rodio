use std::io::BufReader;

fn main() {
    let device = rodio::RodioDevice::default_output().unwrap();
    let sink = rodio::Sink::new(&device);

    let file = std::fs::File::open("examples/music.mp3").unwrap();
    sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());

    sink.sleep_until_end();
}
