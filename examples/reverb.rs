use rodio::Source;
use std::error::Error;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());

    let file = std::fs::File::open("assets/music.ogg")?;
    let source = rodio::Decoder::try_from(file)?;
    let with_reverb = source.buffered().reverb(Duration::from_millis(40), 0.7);
    sink.append(with_reverb);

    sink.sleep_until_end();

    Ok(())
}
