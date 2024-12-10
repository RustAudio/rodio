//! Channel router example

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    use rodio::source::{Function, SignalGenerator, Source};
    use std::thread;
    use std::time::Duration;

    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;

    // let test_signal_duration = Duration::from_millis(1000);
    let interval_duration = Duration::from_millis(100);
    let sample_rate = cpal::SampleRate(48000);

    let (mut controller, router) = SignalGenerator::new(sample_rate, 1000.0, Function::Triangle)
        .amplify(0.1)
        .channel_router(2, vec![vec![0.0f32, 0.0f32]]);

    println!("Playing 1000Hz tone");

    stream_handle.mixer().add(router);

    for i in 0..1000 {
        thread::sleep(interval_duration);
        let n = i % 20;
        match n {
            0 => println!("Left speaker ramp up"),
            1..10 => {
                _ = controller.map(0, 0, n as f32 / 10.0);
                _ = controller.map(0, 1, 0f32);
            }
            10 => println!("Right speaker ramp up"),
            11..20 => {
                _ = controller.map(0, 0, 0.0f32);
                _ = controller.map(0, 1, (n - 10) as f32 / 10.0);
            }
            _ => {}
        }
    }

    Ok(())
}
