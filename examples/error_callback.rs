use cpal::traits::HostTrait;
use rodio::source::SineWave;
use rodio::Source;
use std::error::Error;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    // You can use any other output device that can be queried from CPAL.
    let default_device = cpal::default_host()
        .default_output_device()
        .ok_or("No default audio output device is found.")?;

    let (tx, rx) = std::sync::mpsc::channel();

    let stream_handle = rodio::OutputStreamBuilder::from_device(default_device)?
        .with_error_callback(move |err| {
            // Filter for where err is a DeviceNotAvailable error.
            if let cpal::StreamError::DeviceNotAvailable = err {
                if let Err(e) = tx.send(err) {
                    eprintln!("Error emitting StreamError: {}", e);
                }
            }
        })
        .open_stream_or_fallback()?;

    let mixer = stream_handle.mixer();

    let wave = SineWave::new(740.0)
        .amplify(0.1)
        .take_duration(Duration::from_secs(30));
    mixer.add(wave);

    if let Ok(err) = rx.recv_timeout(Duration::from_secs(30)) {
        // Here we print the error that was emitted by the error callback.
        // but in a real application we may want to destroy the stream and
        // try to reopen it, either with the same device or a different one.
        eprintln!("Error with stream {}", err);
    }

    Ok(())
}
