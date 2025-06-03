//! Plays a tone alternating between right and left ears, with right being first.

use rodio::Source;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());

    let file = std::fs::File::open("assets/RL.ogg")?;
    sink.append(rodio::Decoder::try_from(file)?.amplify(0.2));

    sink.sleep_until_end();

    Ok(())
}
