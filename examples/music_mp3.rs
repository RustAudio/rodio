use std::error::Error;
use std::io::BufReader;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(&stream_handle.mixer());

    let file = std::fs::File::open("assets/music.mp3")?;
    sink.append(rodio::Decoder::new(BufReader::new(file))?);

    sink.sleep_until_end();

    Ok(())
}
