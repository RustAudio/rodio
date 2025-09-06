use std::error::Error;

use rodio::Source;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());

    let path = std::path::Path::new("assets/music.wav");
    let decoder = rodio::Decoder::try_from(path)?;
    let source = decoder.low_pass(200);
    sink.append(source);

    sink.sleep_until_end();

    Ok(())
}
