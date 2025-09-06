use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::DeviceSinkBuilder::open_default_sink()?;
    let player = rodio::Player::connect_new(stream_handle.mixer());

    let path = std::path::Path::new("assets/music.flac");
    player.append(rodio::Decoder::try_from(path)?);

    player.sleep_until_end();

    Ok(())
}
