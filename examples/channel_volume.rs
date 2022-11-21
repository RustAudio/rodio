use std::io::BufReader;

use rodio::source::ChannelVolume;

fn main() {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();

    let file = std::fs::File::open("assets/RL.wav").unwrap();
    let source = rodio::Decoder::new(BufReader::new(file)).unwrap();
    let source = ChannelVolume::new(source, vec![0.5, 1.]);
    sink.append(source);
    sink.sleep_until_end();
}
