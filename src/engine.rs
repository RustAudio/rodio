use std::thread::{self, Builder};
use std::sync::mpsc::{self, Sender, Receiver};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use cpal::Voice;
use decoder::Decoder;

/// The internal engine of this library.
///
/// Each `Engine` owns a thread that runs in the background and plays the audio.
pub struct Engine {
    /// Communication with the background thread.
    commands: Mutex<Sender<Command>>,
    /// Counter that is incremented whenever a sound starts playing and that is used to track each
    /// sound invidiually.
    next_sound_id: AtomicUsize,
}

impl Engine {
    /// Builds the engine.
    pub fn new() -> Engine {
        let (tx, rx) = mpsc::channel();
        // we ignore errors when creating the background thread
        // the user won't get any audio, but that's better than a panic
        let _ = Builder::new().name("rodio audio processing".to_string())
                              .spawn(move || background(rx));
        Engine { commands: Mutex::new(tx), next_sound_id: AtomicUsize::new(1) }
    }

    /// Starts playing a sound and returns a `Handler` to control it.
    pub fn play(&self, decoder: Box<Decoder + Send>) -> Handle {
        let sound_id = self.next_sound_id.fetch_add(1, Ordering::Relaxed);

        let commands = self.commands.lock().unwrap();
        commands.send(Command::Play(sound_id, decoder)).unwrap();

        Handle {
            engine: self,
            id: sound_id,
        }
    }
}

/// Handle to a playing sound.
///
/// Note that dropping the handle doesn't stop the sound. You must call `stop` explicitely.
pub struct Handle<'a> {
    engine: &'a Engine,
    id: usize,
}

impl<'a> Handle<'a> {
    pub fn stop(self) {
        let commands = self.engine.commands.lock().unwrap();
        commands.send(Command::Stop(self.id)).unwrap();
    }
}

pub enum Command {
    Play(usize, Box<Decoder + Send>),
    Stop(usize),
}

fn background(rx: Receiver<Command>) {
    let mut sounds: Vec<(usize, Voice, Box<Decoder + Send>)> = Vec::new();

    loop {
        // polling for new sounds
        if let Ok(command) = rx.try_recv() {
            match command {
                Command::Play(id, decoder) => sounds.push((id, Voice::new(), decoder)),
                Command::Stop(id) => sounds.retain(|&(id2, _, _)| id2 != id),
            }
        }

        // updating the existing sounds
        for &mut (_, ref mut voice, ref mut decoder) in sounds.iter_mut() {
            decoder.write(voice);
            voice.play();
        }

        // sleeping a bit?
        thread::sleep_ms(1);
    }
}
