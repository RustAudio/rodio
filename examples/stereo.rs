//! Plays a tone alternating between right and left ears, with right being first.
use std::io::BufReader;
use rodio::Source;

fn main() {
    let stream_handle = rodio::OutputStreamBuilder::try_default_stream()
        .expect("open default audio stream");
    let sink = rodio::Sink::connect_new(&stream_handle.mixer());    

    let file = std::fs::File::open("assets/RL.ogg").unwrap();
    sink.append(rodio::Decoder::new(BufReader::new(file))
        .unwrap()
        .amplify(0.2));    

    sink.sleep_until_end();
}
