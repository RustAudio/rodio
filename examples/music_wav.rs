extern crate rodio;

fn main() {
    let endpoint = rodio::get_default_endpoint().unwrap();

    let file = std::fs::File::open("examples/music.wav").unwrap();
    let _music = rodio::play_once(&endpoint, file);

    std::thread::sleep_ms(10000);
}
