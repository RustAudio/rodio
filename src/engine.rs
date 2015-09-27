use std::cmp;
use std::mem;
use std::io::{Read, Seek};
use std::thread::{self, Builder, Thread};
use std::sync::mpsc::{self, Sender, Receiver};
use std::sync::Arc;
use std::sync::Mutex;

use cpal::Endpoint;
use decoder;
use decoder::Decoder;

use time;

/// The internal engine of this library.
///
/// Each `Engine` owns a thread that runs in the background and plays the audio.
pub struct Engine {
    /// Communication with the background thread.
    commands: Mutex<Sender<Command>>,

    /// The background thread that executes commands.
    thread: Option<Thread>,
}

impl Engine {
    /// Builds the engine.
    pub fn new() -> Engine {
        let (tx, rx) = mpsc::channel();

        // we ignore errors when creating the background thread
        // the user won't get any audio, but that's better than a panic
        let thread = Builder::new().name("rodio audio processing".to_string())
                                   .spawn(move || background(rx))
                                   .ok().map(|jg| jg.thread().clone());

        Engine {
            commands: Mutex::new(tx),
            thread: thread,
        }
    }

    /// Starts playing a sound and returns a `Handler` to control it.
    pub fn play<R>(&self, endpoint: &Endpoint, input: R) -> Handle
                   where R: Read + Seek + Send + 'static
    {
        let decoder = decoder::decode(endpoint, input);

        let commands = self.commands.lock().unwrap();
        commands.send(Command::Play(decoder.clone())).unwrap();

        if let Some(ref thread) = self.thread {
            thread.unpark();
        }

        Handle {
            engine: self,
            decoder: decoder,
        }
    }
}

/// Handle to a playing sound.
///
/// Note that dropping the handle doesn't stop the sound. You must call `stop` explicitely.
pub struct Handle<'a> {
    engine: &'a Engine,
    decoder: Arc<Mutex<Decoder + Send>>,
}

impl<'a> Handle<'a> {
    #[inline]
    pub fn set_volume(&self, value: f32) {
        // we try to touch the decoder directly from this thread
        if let Ok(mut decoder) = self.decoder.try_lock() {
            decoder.set_volume(value);
        }

        // if `try_lock` failed, that means that the decoder is in use
        // therefore we use the backup plan of sending a message
        let commands = self.engine.commands.lock().unwrap();
        commands.send(Command::SetVolume(self.decoder.clone(), value)).unwrap();
    }

    #[inline]
    pub fn stop(self) {
        let commands = self.engine.commands.lock().unwrap();
        commands.send(Command::Stop(self.decoder)).unwrap();

        if let Some(ref thread) = self.engine.thread {
            thread.unpark();
        }
    }

    #[inline]
    pub fn get_total_duration_ms(&self) -> u32 {
        let decoder = self.decoder.lock().unwrap();
        decoder.get_total_duration_ms()
    }

    #[inline]
    pub fn get_remaining_duration_ms(&self) -> u32 {
        let decoder = self.decoder.lock().unwrap();
        decoder.get_remaining_duration_ms()
    }
}

pub enum Command {
    Play(Arc<Mutex<Decoder + Send>>),
    Stop(Arc<Mutex<Decoder + Send>>),
    SetVolume(Arc<Mutex<Decoder + Send>>, f32),
}

fn background(rx: Receiver<Command>) {
    let mut sounds: Vec<Arc<Mutex<Decoder + Send>>> = Vec::new();
    let mut sounds_to_remove: Vec<Arc<Mutex<Decoder + Send>>> = Vec::new();

    loop {
        // polling for new commands
        if let Ok(command) = rx.try_recv() {
            match command {
                Command::Play(decoder) => {
                    sounds.push(decoder);
                },

                Command::Stop(decoder) => {
                    let decoder = &*decoder as *const _;
                    sounds.retain(|dec| {
                        &**dec as *const _ != decoder
                    })
                },

                Command::SetVolume(decoder, volume) => {
                    let decoder = &*decoder as *const _;
                    if let Some(d) = sounds.iter_mut()
                                           .find(|dec| &***dec as *const _ != decoder)
                    {
                        d.lock().unwrap().set_volume(volume);
                    }
                },
            }
        }

        // removing sounds that have finished playing
        for decoder in mem::replace(&mut sounds_to_remove, Vec::new()) {
            let decoder = &*decoder as *const _;
            sounds.retain(|dec| &**dec as *const _ != decoder)
        }

        let before_updates = time::precise_time_ns();

        // updating the existing sounds
        for decoder in &sounds {
            if !decoder.lock().unwrap().write() {
                sounds_to_remove.push(decoder.clone());
            }
        }

        // sleeping so that we get a loop every 17ms
        let time_taken = time::precise_time_ns() - before_updates;
        let sleep = 17000000u64.saturating_sub(time_taken);
        thread::park_timeout_ms((sleep / 1000000) as u32);
    }
}
