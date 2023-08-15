use std::fs::File;
use std::io::BufReader;

fn main() {
    let new_sr = 44100;
    let channels = 2;
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();

    let file = File::open("assets/voice24khz.mp3").unwrap();
    let source = rodio::Decoder::new(BufReader::new(file)).unwrap();
    let resample: rodio::source::UniformSourceIterator<rodio::Decoder<BufReader<File>>, i16> =
        rodio::source::UniformSourceIterator::new(source, channels, new_sr);

    sink.append(resample);
    sink.sleep_until_end();
}
