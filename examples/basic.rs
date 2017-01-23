extern crate rodio;

use std::io::BufReader;
use std::thread;
use std::time::Duration;

fn main() {
    let endpoint = rodio::get_default_endpoint().unwrap();

    let file = std::fs::File::open("examples/beep.wav").unwrap();
    let mut beep1 = rodio::play_once(&endpoint, BufReader::new(file)).unwrap();
    beep1.set_volume(0.2);
    println!("Beep1 volume: {}", beep1.get_volume());

    thread::sleep(Duration::from_millis(1000));
    beep1.pause();
    thread::sleep(Duration::from_millis(5000));
    beep1.play();
    thread::sleep(Duration::from_millis(9000));
    println!("Beep should end now.");

}
