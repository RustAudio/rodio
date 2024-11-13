use std::io::BufReader;
use std::time::Duration;

fn main() {
    let stream_handle =
        rodio::OutputStreamBuilder::try_default_stream().expect("open default audio stream");
    let sink = rodio::Sink::connect_new(&stream_handle.mixer());

    let file = std::fs::File::open("assets/music.mp3").unwrap();
    sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());

    std::thread::sleep(std::time::Duration::from_secs(2));
    sink.try_seek(Duration::from_secs(0)).unwrap();

    std::thread::sleep(std::time::Duration::from_secs(2));
    sink.try_seek(Duration::from_secs(4)).unwrap();

    sink.sleep_until_end();

    // This doesn't do anything since the sound has ended already.
    sink.try_seek(Duration::from_secs(5)).unwrap();
    println!("seek example ended");
}
