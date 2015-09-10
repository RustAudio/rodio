extern crate rodio;

use std::io::BufReader;

fn main() {
    let endpoint = rodio::get_default_endpoint().unwrap();

    let file = std::fs::File::open("examples/music.ogg").unwrap();
    let _music = rodio::play_once(&endpoint, BufReader::new(file));

    std::thread::sleep_ms(60000);
}
