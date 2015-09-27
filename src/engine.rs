use std::cmp;
use std::mem;
use std::collections::HashMap;
use std::io::{Read, Seek};
use std::thread::{self, Builder, Thread};
use std::sync::mpsc::{self, Sender, Receiver};
use std::sync::Arc;
use std::sync::Mutex;

use cpal::UnknownTypeBuffer;
use cpal::Voice;
use cpal::Endpoint;
use decoder;
use decoder::Decoder;
use conversions::Sample;

use time;

/// Duration of a loop of the engine in milliseconds.
const FIXED_STEP_MS: u32 = 17;
/// Duration of a loop of the engine in nanoseconds.
const FIXED_STEP_NS: u64 = FIXED_STEP_MS as u64 * 1000000;

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
        let decoder = decoder::decode(input, 2, 44100);

        let commands = self.commands.lock().unwrap();
        commands.send(Command::Play(endpoint.clone(), decoder.clone())).unwrap();

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
    decoder: Arc<Mutex<Decoder<Item=f32> + Send>>,
}

impl<'a> Handle<'a> {
    #[inline]
    pub fn set_volume(&self, value: f32) {
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

        let (num_samples, _) = decoder.size_hint();
        //let num_samples = num_samples + self.voice.get_pending_samples();     // TODO: !

        (num_samples as u64 * 1000 / 44100 * 2) as u32          // FIXME: arbitrary values
    }
}

pub enum Command {
    Play(Endpoint, Arc<Mutex<Decoder<Item=f32> + Send>>),
    Stop(Arc<Mutex<Decoder<Item=f32> + Send>>),
    SetVolume(Arc<Mutex<Decoder<Item=f32> + Send>>, f32),
}

fn background(rx: Receiver<Command>) {
    // for each endpoint name, stores the voice and the list of sounds with their volume
    let mut voices: HashMap<String, (Voice, Vec<(Arc<Mutex<Decoder<Item=f32> + Send>>, f32)>)> = HashMap::new();

    // list of sounds to stop playing
    let mut sounds_to_remove: Vec<Arc<Mutex<Decoder<Item=f32> + Send>>> = Vec::new();

    loop {
        // polling for new commands
        if let Ok(command) = rx.try_recv() {
            match command {
                Command::Play(endpoint, decoder) => {
                    let mut entry = voices.entry(endpoint.get_name()).or_insert_with(|| {
                        // TODO: handle possible errors here
                        // TODO: choose format better
                        let format = endpoint.get_supported_formats_list().unwrap().next().unwrap();
                        let voice = Voice::new(&endpoint, &format).unwrap();

                        (voice, Vec::new())
                    });

                    entry.1.push((decoder, 1.0));
                },

                Command::Stop(decoder) => {
                    let decoder = &*decoder as *const _;
                    for (_, &mut (_, ref mut sounds)) in voices.iter_mut() {
                        sounds.retain(|dec| {
                            &*dec.0 as *const _ != decoder
                        })
                    }
                },

                Command::SetVolume(decoder, volume) => {
                    let decoder = &*decoder as *const _;
                    for (_, &mut (_, ref mut sounds)) in voices.iter_mut() {
                        if let Some(d) = sounds.iter_mut()
                                               .find(|dec| &*dec.0 as *const _ == decoder)
                        {
                            d.1 = volume;
                        }
                    }
                },
            }
        }

        // removing sounds that have finished playing
        for decoder in mem::replace(&mut sounds_to_remove, Vec::new()) {
            let decoder = &*decoder as *const _;
            for (_, &mut (_, ref mut sounds)) in voices.iter_mut() {
                sounds.retain(|dec| &*dec.0 as *const _ != decoder);
            }
        }

        // updating the existing sounds
        let before_updates = time::precise_time_ns();
        for (_, &mut (ref mut voice, ref mut sounds)) in voices.iter_mut() {
            // building an iterator that produces samples from `sounds`
            let num_sounds = sounds.len() as f32;
            let samples_iter = (0..).map(|_| {
                // FIXME: locking is slow
                sounds.iter().map(|s| s.0.lock().unwrap().next().unwrap_or(0.0) * s.1 / num_sounds)
                      .fold(0.0, |a, b| a + b)
            });

            // starting the output
            {
                let mut buffer = {
                    let samples_to_write = voice.get_samples_rate().0 * voice.get_channels() as u32 * FIXED_STEP_MS / 1000;
                    voice.append_data(samples_to_write as usize)
                };

                match buffer {
                    UnknownTypeBuffer::U16(ref mut buffer) => {
                        for (o, i) in buffer.iter_mut().zip(samples_iter) { *o = i.to_u16(); }
                    },
                    UnknownTypeBuffer::I16(ref mut buffer) => {
                        for (o, i) in buffer.iter_mut().zip(samples_iter) { *o = i.to_i16(); }
                    },
                    UnknownTypeBuffer::F32(ref mut buffer) => {
                        for (o, i) in buffer.iter_mut().zip(samples_iter) { *o = i; }
                    },
                }
            }

            // TODO: do better
            voice.play();
        }

        // sleeping so that we get a loop every `FIXED_STEP_MS` millisecond
        let time_taken = time::precise_time_ns() - before_updates;
        let sleep = FIXED_STEP_NS.saturating_sub(time_taken);
        thread::park_timeout_ms((sleep / 1000000) as u32);
    }
}
