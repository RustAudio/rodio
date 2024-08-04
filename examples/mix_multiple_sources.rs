use rodio::source::{SineWave, Source};
use rodio::{dynamic_mixer, OutputStream, Sink, queue};
use std::time::Duration;

fn main() {
    // Construct a dynamic controller and mixer, stream_handle, and sink.
    let (controller, mixer) = dynamic_mixer::mixer::<f32>(2, 44_100);
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let sink = Sink::try_new(&stream_handle).unwrap();

    // Create four unique sources. The frequencies used here correspond
    // notes in the key of C and in octave 4: C4, or middle C on a piano,
    // E4, G4, and A4 respectively.

    let notes = vec![261.63, 329.63, 392.0, 440.0];

    notes.into_iter().for_each(|f| {
        let note_source = SineWave::new(f);
        
        let (tx, rx) = queue::queue(false);

        let note_body = note_source
            .clone()
            .take_duration(Duration::from_secs_f32(1.0))
            .amplify(0.20)
            .fade_in(Duration::from_secs_f32(0.1));

        let note_end = note_source
            .clone()
            .skip_duration(Duration::from_secs_f32(1.0))
            .take_duration(Duration::from_secs_f32(1.0))
            .amplify(0.20)
            .linear_gain_ramp(Duration::from_secs_f32(1.0), 1.0, 0.0, true);
        
        tx.append(note_body);
        tx.append(note_end);

        controller.add(rx);
    });

    // Append the dynamic mixer to the sink to play a C major 6th chord.
    sink.append(mixer);

    // Sleep the thread until sink is empty.
    sink.sleep_until_end();
}
