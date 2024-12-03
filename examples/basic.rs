use rodio::source::SineWave;
use rodio::Source;
use std::error::Error;
use std::io::BufReader;
use std::thread;
use std::time::Duration;
#[cfg(feature = "tracing")]
use tracing;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let mixer = stream_handle.mixer();

    let beep1 = {
        // Play a WAV file.
        let file = std::fs::File::open("assets/beep.wav")?;
        let sink = rodio::play(&mixer, BufReader::new(file))?;
        sink.set_volume(0.2);
        sink
    };
    println!("Started beep1");
    thread::sleep(Duration::from_millis(1500));

    {
        // Generate sine wave.
        let wave = SineWave::new(740.0)
            .amplify(0.2)
            .take_duration(Duration::from_secs(3));
        mixer.add(wave);
    }
    println!("Started beep2");
    thread::sleep(Duration::from_millis(1500));

    let beep3 = {
        // Play an OGG file.
        let file = std::fs::File::open("assets/beep3.ogg")?;
        let sink = rodio::play(&mixer, BufReader::new(file))?;
        sink.set_volume(0.2);
        sink
    };
    println!("Started beep3");
    thread::sleep(Duration::from_millis(1500));

    drop(beep1);
    println!("Stopped beep1");

    thread::sleep(Duration::from_millis(1500));
    drop(beep3);
    println!("Stopped beep3");

    thread::sleep(Duration::from_millis(1500));

    Ok(())
}
