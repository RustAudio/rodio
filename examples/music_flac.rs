use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());

    let path = std::path::Path::new("assets/music.flac");
    sink.append(rodio::Decoder::try_from(path)?);

    sink.sleep_until_end();

    Ok(())
}
