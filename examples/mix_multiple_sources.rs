use rodio::mixer;
use rodio::source::{SineWave, Source};
use std::error::Error;
use std::num::NonZero;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    // Construct a dynamic controller and mixer, stream_handle, and sink.
    let (controller, mixer) = mixer::mixer(NonZero::new(2).unwrap(), NonZero::new(44_100).unwrap());
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());

    // Create four unique sources. The frequencies used here correspond
    // notes in the key of C and in octave 4: C4, or middle C on a piano,
    // E4, G4, and A4 respectively.
    let source_c = SineWave::new(261.63)
        .take_duration(Duration::from_secs_f32(1.))
        .amplify(0.20);
    let source_e = SineWave::new(329.63)
        .take_duration(Duration::from_secs_f32(1.))
        .amplify(0.20);
    let source_g = SineWave::new(392.0)
        .take_duration(Duration::from_secs_f32(1.))
        .amplify(0.20);
    let source_a = SineWave::new(440.0)
        .take_duration(Duration::from_secs_f32(1.))
        .amplify(0.20);

    // Add sources C, E, G, and A to the mixer controller.
    controller.add(source_c);
    controller.add(source_e);
    controller.add(source_g);
    controller.add(source_a);

    // Append the dynamic mixer to the sink to play a C major 6th chord.
    sink.append(mixer);

    // Sleep the thread until sink is empty.
    sink.sleep_until_end();

    Ok(())
}
