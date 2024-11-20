use rodio::source::Source;
use rodio::Decoder;
use std::fs::File;
use std::io::BufReader;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const CROSSPLAY_DUR: Duration = Duration::from_secs(5);

fn main() {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let (queue, queue_source) = rodio::queue::queue(true);
    handle.play_raw(queue_source);


    let file = BufReader::new(File::open("assets/music.flac").unwrap());
    let track = Decoder::new(file).unwrap().buffered();
    let Some(track_dur) = track.total_duration() else {
        panic!("can not support crossfade if we do not know when the source will end");
        // would be nice if we could buffer the first n-duration of the source
        // then we do not need to know the duration at all
    };

    let until_crossplay = track_dur.saturating_sub(CROSSPLAY_DUR);
    let track_except_end = track.clone().take_duration(until_crossplay);
    let track_end = track.clone().skip_duration(until_crossplay);

    let track_except_end = queue.append(track_except_end);
    let track_end = queue.append(track_end);

    let file = BufReader::new(File::open("assets/music.wav").unwrap());
    let new_track = Decoder::new(file).unwrap();

    let until_crossplay = track_dur.saturating_sub(CROSSPLAY_DUR);
    let track_except_end = track.clone().take_duration(until_crossplay);
    let track_end = track.clone().skip_duration(until_crossplay);
}
