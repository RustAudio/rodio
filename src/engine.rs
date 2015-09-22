use std::cmp;
use std::io::{Read, Seek};
use std::thread::{self, Builder, Thread};
use std::sync::mpsc::{self, Sender, Receiver};
use std::sync::atomic::{AtomicUsize, Ordering};
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
        let thread = Builder::new().name("rodio audio processing".to_string())
                                   .spawn(move || background(rx))
                                   .ok().map(|jg| jg.thread().clone());

        Engine {
            commands: Mutex::new(tx),
            thread: thread,
            next_sound_id: AtomicUsize::new(1),
        }
    }

    /// Starts playing a sound and returns a `Handler` to control it.
    pub fn play<R>(&self, endpoint: &Endpoint, input: R) -> Handle
                   where R: Read + Seek + Send + 'static
    {
        let decoder = decoder::decode(endpoint, input);

        let sound_id = self.next_sound_id.fetch_add(1, Ordering::Relaxed);
        let commands = self.commands.lock().unwrap();
        commands.send(Command::Play(sound_id, decoder)).unwrap();

        if let Some(ref thread) = self.thread {
            thread.unpark();
        }

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
    #[inline]
    pub fn set_volume(&mut self, value: f32) {
        let commands = self.engine.commands.lock().unwrap();
        commands.send(Command::SetVolume(self.id, value)).unwrap();

        // we do not wake up the commands thread
        // the samples with the previous volume have already been submitted, therefore it won't
        // change anything if we wake it up
    }

    #[inline]
    pub fn stop(self) {
        let commands = self.engine.commands.lock().unwrap();
        commands.send(Command::Stop(self.id)).unwrap();

        if let Some(ref thread) = self.engine.thread {
            thread.unpark();
        }
    }
}

pub enum Command {
    Play(usize, Box<Decoder + Send>),
    Stop(usize),
    SetVolume(usize, f32),
}

fn background(rx: Receiver<Command>) {
    let mut sounds: Vec<(usize, Box<Decoder + Send>)> = Vec::new();

    loop {
        // polling for new sounds
        if let Ok(command) = rx.try_recv() {
            match command {
                Command::Play(id, decoder) => {
                    sounds.push((id, decoder));
                },

                Command::Stop(id) => {
                    sounds.retain(|&(id2, _)| id2 != id)
                },

                Command::SetVolume(id, volume) => {
                    if let Some(&mut (_, ref mut d)) = sounds.iter_mut()
                                                             .find(|&&mut (i, _)| i == id)
                    {
                        d.set_volume(volume);
                    }
                },
            }
        }

        let before_updates = time::precise_time_ns();

        // stores the time when this thread will have to be woken up
        let mut next_step_ns = before_updates + 1000000000;   // 1s

        // updating the existing sounds
        for &mut (_, ref mut decoder) in &mut sounds {
            let val = decoder.write();
            let val = time::precise_time_ns() + val;
            next_step_ns = cmp::min(next_step_ns, val);     // updating next_step_ns
        }

        // time taken to run the updates
        let after_updates = time::precise_time_ns();
        let updates_time_taken = after_updates - before_updates;

        // sleeping a bit if we can
        let sleep = next_step_ns as i64 - after_updates as i64;
        // the sleep duration is equal
        // to `time_until_overflow - time_it_takes_to_write_data - 200µs`
        // we remove 200µs to handle variations in the time it takes to write
        let sleep = sleep - updates_time_taken as i64 - 200000;
        if sleep >= 0 {
            thread::park_timeout_ms((sleep / 1000000) as u32);
        }
    }
}
