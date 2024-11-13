//! Noise generator example. Use the "noise" feature to enable the noise generator sources.

use std::io::BufReader;

#[cfg(feature = "noise")]
fn main() {
    use rodio::source::{pink, white, Source};
    use std::thread;
    use std::time::Duration;

    let stream_handle =
        rodio::OutputStreamBuilder::try_default_stream().expect("open default audio stream");

    let noise_duration = Duration::from_millis(1000);
    let interval_duration = Duration::from_millis(1500);

    stream_handle.mixer().add(
        white(cpal::SampleRate(48000))
            .amplify(0.1)
            .take_duration(noise_duration),
    );
    println!("Playing white noise");

    thread::sleep(interval_duration);

    stream_handle.mixer().add(
        pink(cpal::SampleRate(48000))
            .amplify(0.1)
            .take_duration(noise_duration),
    );
    println!("Playing pink noise");

    thread::sleep(interval_duration);
}

#[cfg(not(feature = "noise"))]
fn main() {
    println!("rodio has not been compiled with noise sources, use `--features noise` to enable this feature.");
    println!("Exiting...");
}
