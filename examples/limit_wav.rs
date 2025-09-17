use rodio::{source::LimitSettings, Source};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());

    let path = std::path::Path::new("assets/music.wav");
    let source = rodio::Decoder::try_from(path)?
        .amplify(3.0)
        .limit(LimitSettings::default());

    sink.append(source);

    println!("Playing music.wav with limiting until finished...");
    sink.sleep_until_end();
    println!("Done.");

    Ok(())
}
