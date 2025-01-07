//! Channel router example

use std::io::Read;
use std::{error::Error, io};

fn main() -> Result<(), Box<dyn Error>> {
    use rodio::source::{Function, SignalGenerator, Source};

    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;

    let sample_rate: u32 = 48000;

    let (controller, router) = SignalGenerator::new(sample_rate, 440.0, Function::Triangle)
        .amplify(0.1)
        .channel_router(2, &vec![]);

    println!("Control left and right levels separately:");
    println!("q: left+\na: left-\nw: right+\ns: right-\nx: quit");

    stream_handle.mixer().add(router);

    let (mut left_level, mut right_level) = (0.5f32, 0.5f32);
    controller.set_map(&vec![(0, 0, left_level), (0, 1, right_level)])?;
    println!("Left: {left_level:.04}, Right: {right_level:.04}");

    let bytes = io::stdin().bytes();
    for chr in bytes {
        match chr.unwrap() {
            b'q' => left_level += 0.1,
            b'a' => left_level -= 0.1,
            b'w' => right_level += 0.1,
            b's' => right_level -= 0.1,
            b'x' => break,
            b'\n' => {
                left_level = left_level.clamp(0.0, 1.0);
                right_level = right_level.clamp(0.0, 1.0);
                controller.set_map(&vec![(0, 0, left_level), (0, 1, right_level)])?;
                println!("Left: {left_level:.04}, Right: {right_level:.04}");
            }
            _ => continue,
        }
    }

    Ok(())
}
