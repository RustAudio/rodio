use std::io::BufReader;

fn main() {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();

    let file = std::fs::File::open("examples/music.mp3").unwrap();
    sink.append_seekable(rodio::Decoder::new(BufReader::new(file)).unwrap());

    loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        sink.set_pos(2.0);
        dbg!("setting pos");
    }

    sink.sleep_until_end();
}
