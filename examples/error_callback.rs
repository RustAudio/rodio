use cpal::traits::HostTrait;
use rodio::source::SineWave;
use rodio::Source;
use std::error::Error;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    // You can use any other output device that can be queried from CPAL.
    let default_device = cpal::default_host()
        .default_output_device()
        .ok_or("No default audio output device is found.")?;

    let stream_handle = rodio::OutputStreamBuilder::from_device(default_device)?
        .with_error_callback(|err| {
            // Filter for where err is a DeviceNotAvailable error.
            if let cpal::StreamError::DeviceNotAvailable = err {
                eprintln!("The audio device is not available.");
            }
        })
        .open_stream_or_fallback()?;

    let mixer = stream_handle.mixer();

    let wave = SineWave::new(740.0)
        .amplify(0.1)
        .take_duration(Duration::from_secs(1));
    mixer.add(wave);

    println!("Beep...");
    thread::sleep(Duration::from_millis(1500));

    Ok(())
}
