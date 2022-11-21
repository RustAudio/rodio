use std::io::BufReader;
use std::thread;
use std::time::Duration;

use rodio::Source;

fn main() {
    let iter_duration = Duration::from_secs(5);
    let iter_distance = 5.;

    let refresh_duration = Duration::from_millis(10);

    let num_steps = iter_duration.as_secs_f32() / refresh_duration.as_secs_f32();
    let step_distance = iter_distance / num_steps;
    let num_steps = num_steps as u32;

    let repeats = 5;

    let total_duration = iter_duration * 2 * repeats;

    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let mut positions = ([0., 0., 0.], [-1., 0., 0.], [1., 0., 0.]);
    let sink = rodio::SpatialSink::try_new(&handle, positions.0, positions.1, positions.2).unwrap();

    let file = std::fs::File::open("assets/music.ogg").unwrap();
    let source = rodio::Decoder::new(BufReader::new(file))
        .unwrap()
        .repeat_infinite()
        .take_duration(total_duration);
    sink.append(source);
    // A sound emitter playing the music starting at the centre gradually moves to the right
    // until it stops and begins traveling to the left, it will eventually pass through the
    // listener again and go to the far left.
    // This is repeated 5 times.
    for _ in 0..repeats {
        for _ in 0..num_steps {
            thread::sleep(refresh_duration);
            positions.0[0] += step_distance;
            sink.set_emitter_position(positions.0);
        }
        for _ in 0..(num_steps * 2) {
            thread::sleep(refresh_duration);
            positions.0[0] -= step_distance;
            sink.set_emitter_position(positions.0);
        }
        for _ in 0..num_steps {
            thread::sleep(refresh_duration);
            positions.0[0] += step_distance;
            sink.set_emitter_position(positions.0);
        }
    }
    sink.sleep_until_end();
}
