use rodio::Source;
use std::error::Error;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());

    let file = std::fs::File::open("assets/music.wav")?;
    let source = rodio::Decoder::try_from(file)?;

    // Shared flag to enable/disable compressor
    let compressor_enabled = Arc::new(AtomicBool::new(true));
    let compressor_enabled_clone = compressor_enabled.clone();

    // Apply compressor and alternate the effect during playback
    let compressed = source.compressor(0.01, 20.0, 0.0005, 0.02).periodic_access(
        Duration::from_millis(250),
        move |src| {
            let enable = compressor_enabled_clone.load(Ordering::Relaxed);
            if enable {
                src.set_threshold(0.01);
                src.set_ratio(20.0);
                src.set_attack(0.0005);
                src.set_release(0.02);
            } else {
                src.set_threshold(1.0); // effectively disables compression
                src.set_ratio(1.0);
                src.set_attack(0.0005);
                src.set_release(0.02);
            }
        },
    );

    sink.append(compressed);

    println!("Playing music.wav with alternating compressor effect...");
    // Alternate the compressor effect every two seconds for 10 cycles
    for _ in 0..10 {
        thread::sleep(Duration::from_secs(2));
        let prev = compressor_enabled.load(Ordering::Relaxed);
        compressor_enabled.store(!prev, Ordering::Relaxed);
        println!("Compressor {}", if !prev { "ON" } else { "OFF" });
    }

    // Wait for playback to finish
    sink.sleep_until_end();
    println!("Done.");

    Ok(())
}
