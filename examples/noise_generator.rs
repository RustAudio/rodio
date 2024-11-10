//! Noise generator example. Use the "noise" feature to enable the noise generator sources.

#[cfg(feature = "noise")]
fn main() {
    use rodio::source::{pink, white, Source};
    use std::thread;
    use std::time::Duration;

    let (_stream, stream_handle) = rodio::OutputStream::try_default().unwrap();

    let noise_duration = Duration::from_millis(1000);
    let interval_duration = Duration::from_millis(1500);

    stream_handle
        .play_raw(
            white(cpal::SampleRate(48000))
                .amplify(0.1)
                .take_duration(noise_duration),
        )
        .unwrap();
    println!("Playing white noise");

    thread::sleep(interval_duration);

    stream_handle
        .play_raw(
            pink(cpal::SampleRate(48000))
                .amplify(0.1)
                .take_duration(noise_duration),
        )
        .unwrap();
    println!("Playing pink noise");

    thread::sleep(interval_duration);
}

#[cfg(not(feature = "noise"))]
fn main() {
    println!("rodio has not been compiled with noise sources, use `--features noise` to enable this feature.");
    println!("Exiting...");
}
