use std::io::BufReader;
use std::thread;
use std::time::Duration;

fn main() {
    let stream = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::SpatialSink::new(
        &stream,
        [-10.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
    );

    let file = std::fs::File::open("examples/music.ogg").unwrap();
    let source = rodio::Decoder::new(BufReader::new(file)).unwrap();
    sink.append(source);

    // A sound emitter playing the music starting at the left gradually moves to the right
    // eventually passing through the listener, then it continues on to the right for a distance
    // until it stops and begins traveling to the left, it will eventually pass through the
    // listener again.
    // This is repeated 5 times.
    for _ in 0..5 {
        for i in 1..1001 {
            thread::sleep(Duration::from_millis(5));
            sink.set_emitter_position([(i - 500) as f32 / 50.0, 0.0, 0.0]);
        }
        for i in 1..1001 {
            thread::sleep(Duration::from_millis(5));
            sink.set_emitter_position([-(i - 500) as f32 / 50.0, 0.0, 0.0]);
        }
    }
    sink.sleep_until_end();
}
