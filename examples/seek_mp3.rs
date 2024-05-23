use std::io::BufReader;
use std::time::Duration;

fn main() {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();

    let file = std::fs::File::open("assets/music.mp3").unwrap();
    sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());

    std::thread::sleep(std::time::Duration::from_secs(2));
    sink.try_seek(Duration::from_secs(0)).unwrap();

    std::thread::sleep(std::time::Duration::from_secs(2));
    sink.try_seek(Duration::from_secs(4)).unwrap();

    sink.sleep_until_end();

    // wont do anything since the sound has ended already
    sink.try_seek(Duration::from_secs(5)).unwrap();
    println!("seek example ended");
}
