extern crate rodio;

use std::io::BufReader;

fn main() {
    let endpoint = rodio::default_output_device().unwrap();
    let sink = rodio::Sink::new(&endpoint);

    let file = std::fs::File::open("examples/music.ogg").unwrap();
    sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());

    sink.sleep_until_end();
}
