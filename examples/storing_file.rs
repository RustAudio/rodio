use rodio::source::{Buffered, SkipDuration, TakeDuration};
use rodio::{source::Source, Decoder, Sink};
use std::fs::File;
use std::io::BufReader;
use std::time::Duration;

fn main() {
    let (_stream, stream_handle) = rodio::OutputStream::try_default().unwrap();

    // Loading an audio file and storing it to use again
    let file = BufReader::new(File::open("assets/beep.wav").unwrap());
    let buffered_beep = Decoder::new(file).unwrap().buffered();

    // Modifying loaded audio file
    let buffered_modified_beep: Buffered<
        TakeDuration<SkipDuration<Buffered<Decoder<BufReader<File>>>>>,
    >;
    buffered_modified_beep = buffered_beep
        .clone()
        .skip_duration(Duration::from_millis(10))
        .take_duration(Duration::from_millis(1000))
        .buffered();

    let sink = Sink::try_new(&stream_handle).unwrap();

    sink.append(buffered_modified_beep.clone());
    sink.sleep_until_end();

    sink.append(buffered_modified_beep.clone());
    sink.sleep_until_end();
}
