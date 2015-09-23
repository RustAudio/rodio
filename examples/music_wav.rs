extern crate rodio;

use std::io::BufReader;

fn main() {
    let endpoint = rodio::get_default_endpoint().unwrap();

    let file = std::fs::File::open("examples/music.wav").unwrap();
    let music = rodio::play_once(&endpoint, BufReader::new(file));

    music.sleep_until_end();
}
