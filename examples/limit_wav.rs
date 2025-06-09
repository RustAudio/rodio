use rodio::{source::LimitSettings, Source};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());

    let file = std::fs::File::open("assets/music.wav")?;
    let source = rodio::Decoder::try_from(file)?
        .amplify(3.0)
        .limit(LimitSettings::default());

    sink.append(source);

    println!("Playing music.wav with limiting until finished...");
    sink.sleep_until_end();
    println!("Done.");

    Ok(())
}
