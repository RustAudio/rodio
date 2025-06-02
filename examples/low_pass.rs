#![cfg_attr(not(feature = "playback"), allow(unused_imports))]

use std::error::Error;
use std::io::BufReader;

use rodio::Source;

#[cfg(feature = "playback")]
fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());

    let file = std::fs::File::open("assets/music.wav")?;
    let decoder = rodio::Decoder::new(BufReader::new(file))?;
    let source = decoder.low_pass(200);
    sink.append(source);

    sink.sleep_until_end();

    Ok(())
}

#[cfg(not(feature = "playback"))]
fn main() {
    println!("rodio has not been compiled with playback, use `--features playback` to enable this feature.");
    println!("Exiting...");
}
