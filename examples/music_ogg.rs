use std::{error::Error, io::Cursor};

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());

    let file = include_bytes!("../assets/music.ogg");
    let cursor = Cursor::new(file);
    sink.append(rodio::Decoder::try_from(cursor)?);

    sink.sleep_until_end();

    Ok(())
}
