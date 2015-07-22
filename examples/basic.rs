extern crate rodio;

fn main() {
    let file = std::fs::File::open("examples/beep.wav").unwrap();

    rodio::play_once(file);

    std::thread::sleep_ms(10000);
}
