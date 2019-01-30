extern crate rodio;

use rodio::Source;
use std::io::BufReader;
use std::time::{Duration};
use std::sync::{Mutex, Arc};
use std::thread;

fn main() {
    let device = rodio::default_output_device().unwrap();
    let sink = rodio::Sink::new(&device);

    let file = std::fs::File::open("examples/music.ogg").unwrap();
    let source = rodio::Decoder::new(BufReader::new(file)).unwrap();

    let timer = Arc::new(Mutex::new(Duration::from_secs(0)));
    let with_elapsed = source.buffered().elapsed(Arc::clone(&timer));
    sink.append(with_elapsed);

    while !sink.empty() {
        let val = *timer.lock().unwrap();

        println!("Music has played for {} seconds", val.as_secs());
        thread::sleep(Duration::from_secs(1));
    }
}
