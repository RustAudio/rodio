use std::error::Error;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());

    let file = std::fs::File::open("assets/music.mp3")?;
    sink.append(rodio::Decoder::try_from(file)?);

    std::thread::sleep(std::time::Duration::from_secs(2));
    sink.try_seek(Duration::from_secs(0))?;

    std::thread::sleep(std::time::Duration::from_secs(2));
    sink.try_seek(Duration::from_secs(4))?;

    sink.sleep_until_end();

    // This doesn't do anything since the sound has ended already.
    sink.try_seek(Duration::from_secs(5))?;
    println!("seek example ended");

    Ok(())
}
