extern crate rodio;

use std::io::BufReader;
use std::thread;
use std::time::Duration;

fn main() {
    let endpoint = rodio::get_default_endpoint().unwrap();
    let sink = rodio::Sink::new(&endpoint);

    let file = std::fs::File::open("examples/music.ogg").unwrap();
    sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());

    thread::sleep(Duration::from_millis(60000));
}
