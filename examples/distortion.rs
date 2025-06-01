use rodio::source::Source;
use rodio::Decoder;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    // Open the default output stream and get the mixer
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let mixer = stream_handle.mixer();

    // Open and decode the MP3 file
    let file = File::open("assets/music.mp3")?;
    let source = Decoder::try_from(BufReader::new(file))?;

    // Apply distortion effect
    let distorted = source
        .distortion(4.0, 0.3)
        .take_duration(Duration::from_secs(5));

    // Play the distorted sound
    mixer.add(distorted);

    println!("Playing music.mp3 with distortion for 5 seconds...");
    thread::sleep(Duration::from_secs(5));
    println!("Done.");

    Ok(())
}
