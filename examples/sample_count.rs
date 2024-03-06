use rodio::Decoder;
use std::fs::File;
use std::io::BufReader;

fn main() {
    let (_stream, stream_handle) = rodio::OutputStream::try_default().unwrap();

    let file = BufReader::new(File::open("assets/music.flac").unwrap());
    let decoder = Decoder::new(file).unwrap();
    let info = decoder.get_info();
    let sink = rodio::Sink::try_new(&stream_handle).unwrap();
    sink.append(decoder);
    while sink.len() > 0 {
        println!("Elapsed Duration: {:?}", info.elapsed_duration().unwrap());
    }
}
