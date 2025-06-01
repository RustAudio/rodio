use std::error::Error;

use rodio::Source;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());

    let file = std::fs::File::open("assets/music.wav")?;
    // Apply distortion effect before appending to the sink
    let source = rodio::Decoder::try_from(file)?.distortion(4.0, 0.3);
    sink.append(source);

    sink.sleep_until_end();

    Ok(())
}
