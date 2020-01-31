extern crate rodio;

use rodio::Source;
use std::io::BufReader;
use std::time::Duration;

fn main() {
    let device = rodio::RodioDevice::default_output().unwrap();
    let sink = rodio::Sink::new(&device);

    let file = std::fs::File::open("examples/music.ogg").unwrap();
    let source = rodio::Decoder::new(BufReader::new(file)).unwrap();
    let with_reverb = source.buffered().reverb(Duration::from_millis(40), 0.7);
    sink.append(with_reverb);

    sink.sleep_until_end();
}
