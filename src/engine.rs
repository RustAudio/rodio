use std::thread;
use std::sync::mpsc::{self, Sender, Receiver};
use std::sync::Mutex;

use cpal::Voice;
use decoder::Decoder;

pub enum Command {
    Play(Box<Decoder + Send>)
}

pub struct Engine {
    commands: Mutex<Sender<Command>>,
}

impl Engine {
    pub fn new() -> Engine {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || background(rx));
        Engine { commands: Mutex::new(tx) }
    }

    pub fn play_once(&self, decoder: Box<Decoder + Send>) {
        let commands = self.commands.lock().unwrap();
        commands.send(Command::Play(decoder)).unwrap();
    }
}

fn background(rx: Receiver<Command>) {
    let mut sounds = Vec::new();

    loop {
        // polling for new sounds
        if let Ok(command) = rx.try_recv() {
            match command {
                Command::Play(decoder) => sounds.push((Voice::new(), decoder)),
            }
        }

        for &mut (ref mut voice, ref mut decoder) in sounds.iter_mut() {
            decoder.write(voice);
            voice.play();
        }
    }
}
