use rodio::output_to_wav;
use std::error::Error;
use std::io::BufReader;

/// Converts mp3 file to a wav file.
/// This example does not use any audio devices
/// and can be used in build configurations without `cpal` feature enabled.
fn main() -> Result<(), Box<dyn Error>> {
    let file = std::fs::File::open("assets/music.mp3")?;
    let mut audio = rodio::Decoder::new(BufReader::new(file))?;

    let wav_path = "music_mp3_converted.wav";
    println!("Storing converted audio into {}", wav_path);
    output_to_wav(&mut audio, wav_path)?;

    Ok(())
}
