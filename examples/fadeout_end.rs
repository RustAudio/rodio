use std::error::Error;
use std::io::BufReader;
use std::time::Duration;

use rodio::Source;

fn main() -> Result<(), Box<dyn Error>> {
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(&stream_handle.mixer());

    let file = std::fs::File::open("assets/music.wav")?;
    let music = rodio::Decoder::new(BufReader::new(file))?;
    let [start, end] = music.split_once(Duration::from_secs(3));
    let end_duration = end
        .total_duration()
        .expect("can only fade out at the end if we know the total duration");
    let end = end.fade_out(end_duration);

    sink.append(start);
    sink.append(end);

    sink.sleep_until_end();

    Ok(())
}
