use rodio::source::Source;
use std::error::Error;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::DeviceSinkBuilder::open_default_sink()?;
    let player = rodio::Player::connect_new(stream_handle.mixer());

    // Generate a 440 Hz sine wave and wrap it in the loudness meter.
    let source = rodio::source::SineWave::new(440.0)
        .take_duration(Duration::from_secs(10))
        .loudness();

    // periodic_access lets us read loudness readings while the audio plays.
    let metered = source.periodic_access(Duration::from_millis(500), |src| {
        println!(
            "momentary: {:.1} LUFS  short-term: {:.1} LUFS  integrated: {:.1} LUFS",
            src.momentary_lufs(),
            src.short_term_lufs(),
            src.integrated_lufs(),
        );
    });

    player.append(metered);
    player.sleep_until_end();
    Ok(())
}
