use rodio::{source::LimitSettings, Source};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::DeviceSinkBuilder::open_default_sink()?;
    let player = rodio::Player::connect_new(stream_handle.mixer());

    let path = std::path::Path::new("assets/music.wav");
    let source = rodio::Decoder::try_from(path)?
        .amplify(3.0)
        .limit(LimitSettings::default());

    player.append(source);

    println!("Playing music.wav with limiting until finished...");
    player.sleep_until_end();
    println!("Done.");

    Ok(())
}
