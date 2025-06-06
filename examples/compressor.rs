use rodio::source::{SineWave, Source};
use std::error::Error;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    // Open the default output stream and get the mixer
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let mixer = stream_handle.mixer();

    // Create a sine wave source and apply compressor
    let compressed = SineWave::new(440.0)
        .amplify(0.5)
        .compressor(0.2, 4.0, 0.01, 0.1)
        .take_duration(Duration::from_secs(3));

    // Play the compressed sound
    mixer.add(compressed);

    println!("Playing compressed sine wave for 3 seconds...");
    thread::sleep(Duration::from_secs(3));
    println!("Done.");

    Ok(())
}
