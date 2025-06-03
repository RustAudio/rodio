//! Noise generator example. Use the "noise" feature to enable the noise generator sources.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    use rodio::source::{pink, white, Source};
    use std::thread;
    use std::time::Duration;

    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;

    let noise_duration = Duration::from_millis(1000);
    let interval_duration = Duration::from_millis(1500);

    stream_handle
        .mixer()
        .add(white(48000).amplify(0.1).take_duration(noise_duration));
    println!("Playing white noise");

    thread::sleep(interval_duration);

    stream_handle
        .mixer()
        .add(pink(48000).amplify(0.1).take_duration(noise_duration));
    println!("Playing pink noise");

    thread::sleep(interval_duration);

    Ok(())
}
