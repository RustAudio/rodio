use rodio::Source;
use std::io::BufReader;
use std::time::Duration;

fn main() {
    let stream_handle =
        rodio::OutputStreamBuilder::try_default_stream().expect("open default audio stream");
    let sink = rodio::Sink::connect_new(&stream_handle.mixer());

    let file = std::fs::File::open("assets/music.ogg").unwrap();
    let source = rodio::Decoder::new(BufReader::new(file)).unwrap();
    let with_reverb = source.buffered().reverb(Duration::from_millis(40), 0.7);
    sink.append(with_reverb);

    sink.sleep_until_end();
}
