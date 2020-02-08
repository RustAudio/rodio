use std::io::BufReader;
use std::thread;
use std::time::Duration;

fn main() {
    let stream = rodio::OutputStream::try_default().unwrap();

    let file = std::fs::File::open("examples/beep.wav").unwrap();
    let beep1 = stream.play_once(BufReader::new(file)).unwrap();
    beep1.set_volume(0.2);
    println!("Started beep1");

    thread::sleep(Duration::from_millis(1500));

    let file = std::fs::File::open("examples/beep2.wav").unwrap();
    let beep2 = stream.play_once(BufReader::new(file)).unwrap();
    beep2.set_volume(0.3);
    beep2.detach();
    println!("Started beep2");

    thread::sleep(Duration::from_millis(1500));
    let file = std::fs::File::open("examples/beep3.ogg").unwrap();
    let beep3 = stream.play_once(file).unwrap();
    beep3.set_volume(0.2);
    println!("Started beep3");

    thread::sleep(Duration::from_millis(1500));
    drop(beep1);
    println!("Stopped beep1");

    thread::sleep(Duration::from_millis(1500));
    drop(beep3);
    println!("Stopped beep3");

    thread::sleep(Duration::from_millis(1500));
}
