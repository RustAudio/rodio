use std::thread;
use std::sync::mpsc::{self, Sender, Receiver};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use cpal::Voice;
use decoder::Decoder;

pub struct Engine {
    /// Communication with the background thread.
    commands: Mutex<Sender<Command>>,
    /// Counter that is incremented whenever a sound starts playing.
    sound_ids: AtomicUsize,
}

impl Engine {
    pub fn new() -> Engine {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || background(rx));
        Engine { commands: Mutex::new(tx), sound_ids: AtomicUsize::new(0) }
    }

    pub fn play(&self, decoder: Box<Decoder + Send>) -> Handle {
        let sound_id = self.sound_ids.fetch_add(1, Ordering::Relaxed);

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
    let mut sounds = Vec::new();

    loop {
        // polling for new sounds
        if let Ok(command) = rx.try_recv() {
            match command {
                Command::Play(id, decoder) => sounds.push((id, Voice::new(), decoder)),
                Command::Stop(id) => sounds.retain(|&(id2, _, _)| id2 != id),
            }
        }

        for &mut (_, ref mut voice, ref mut decoder) in sounds.iter_mut() {
            decoder.write(voice);
            voice.play();
        }
    }
}
