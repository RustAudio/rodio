extern crate rodio;

fn main() {
    let file = std::fs::File::open("examples/beep.wav").unwrap();
    let beep1 = rodio::play_once(file);

    std::thread::sleep_ms(1000);

    let file = std::fs::File::open("examples/beep2.wav").unwrap();
    rodio::play_once(file);

    std::thread::sleep_ms(1000);
    beep1.stop();

    std::thread::sleep_ms(8000);
}
