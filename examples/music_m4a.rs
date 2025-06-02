#![cfg_attr(not(feature = "playback"), allow(unused_imports))]

use std::error::Error;

#[cfg(feature = "playback")]
fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(&stream_handle.mixer());

    let file = std::fs::File::open("assets/music.m4a")?;
    sink.append(rodio::Decoder::try_from(file)?);

    sink.sleep_until_end();

    Ok(())
}

#[cfg(not(feature = "playback"))]
fn main() {
    println!("rodio has not been compiled with playback, use `--features playback` to enable this feature.");
    println!("Exiting...");
}
