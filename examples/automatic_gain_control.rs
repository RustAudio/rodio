use rodio::source::Source;
use rodio::Decoder;
use std::fs::File;
use std::io::BufReader;

fn main() {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::Sink::try_new(&handle).unwrap();

    let file = BufReader::new(File::open("assets/music.flac").unwrap());

    // Decode the sound file into a source
    let source = Decoder::new(file).unwrap();

    // Apply automatic gain control to the source
    let agc_source = source.automatic_gain_control(1.0, 4.0, 0.005, 5.0);

    // Get a handle to control the AGC's enabled state (only when using experimental feature)
    let agc_control = agc_source.get_agc_control();

    // Disable AGC by default when using experimental feature
    agc_control.store(true, std::sync::atomic::Ordering::Relaxed);

    // Add the AGC-processed source to the sink for playback
    sink.append(agc_source);

    // Keep the program running until playback is complete
    sink.sleep_until_end();
}
