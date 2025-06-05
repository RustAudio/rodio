use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());
    drop(stream_handle); // audio output not yet closed since sink is still alive

    let file = std::fs::File::open("assets/music.flac")?;
    sink.append(rodio::Decoder::try_from(file)?);
    sleep(Duration::from_secs(2));

    drop(sink);
    println!(
        "Audio output was closed because the last \
        reference to it (Sink) was dropped"
    );
    sleep(Duration::from_secs(2));
    Ok(())
}
