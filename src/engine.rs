use std::cmp;
use std::mem;
use std::collections::HashMap;
use std::io::{Read, Seek};
use std::thread::{self, Builder, Thread};
use std::sync::mpsc::{self, Sender, Receiver};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;

use cpal;
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

    /// Contains the format (channels count and samples rate) of the voice of each endpoint.
    voices_formats: Mutex<HashMap<String, (u16, u32)>>,
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
            voices_formats: Mutex::new(HashMap::new()),
        }
    }

    /// Starts playing a sound and returns a `Handler` to control it.
    pub fn play<R>(&self, endpoint: &Endpoint, input: R) -> Handle
                   where R: Read + Seek + Send + 'static
    {
        // try looking for an existing voice, or create one if there isn't one
        let (new_voice, channels_count, samples_rate) = {
            let mut voices_formats = self.voices_formats.lock().unwrap();
            let mut new_voice = None;

            let &mut (c, s) = voices_formats.entry(endpoint.get_name()).or_insert_with(|| {
                // TODO: handle possible errors here
                let format = endpoint.get_supported_formats_list().unwrap().fold(None, |f1, f2| {
                    if f1.is_none() {
                        return Some(f2);
                    }

                    let f1 = f1.unwrap();

                    // we privilege f32 formats to avoid a conversion
                    if f1.data_type == cpal::SampleFormat::F32 && f2.data_type != cpal::SampleFormat::F32 {
                        return Some(f1);
                    }
                    if f2.data_type == cpal::SampleFormat::F32 && f1.data_type != cpal::SampleFormat::F32 {
                        return Some(f2);
                    }

                    if f1.channels.len() < f2.channels.len() {
                        return Some(f2);
                    }
                    if f2.channels.len() < f1.channels.len() {
                        return Some(f1);
                    }

                    if f1.samples_rate.0 < 44100 && f2.samples_rate.0 >= 44100 {
                        return Some(f2);
                    }
                    if f2.samples_rate.0 < 44100 && f1.samples_rate.0 >= 44100 {
                        return Some(f1);
                    }

                    Some(f1)
                }).expect("The endpoint doesn't support any format!?");

                new_voice = Some(Voice::new(&endpoint, &format).unwrap());
                (format.channels.len() as u16, format.samples_rate.0)
            });

            (new_voice, c, s)
        };

        // try build the decoder
        let decoder = decoder::decode(input, channels_count, samples_rate);
        let decoder_id = &*decoder as *const _ as *const u8 as usize;

        // getting some infos ; we are going to send the decoder to the background thread, so this
        // is the last time we can get infos from it
        let total_duration_ms = decoder.get_total_duration_ms();

        // at each loop, the background thread will store the remaining time of the sound in this
        // value
        let remaining_duration_ms = Arc::new(AtomicUsize::new(total_duration_ms as usize));

        // send the play command
        let commands = self.commands.lock().unwrap();
        commands.send(Command::Play(endpoint.clone(), new_voice, decoder, remaining_duration_ms.clone())).unwrap();

        // unpark the background thread so that the sound starts playing immediately
        if let Some(ref thread) = self.thread {
            thread.unpark();
        }

        Handle {
            engine: self,
            decoder_id: decoder_id,
            total_duration_ms: total_duration_ms,
            remaining_duration_ms: remaining_duration_ms,
        }
    }
}

/// Handle to a playing sound.
///
/// Note that dropping the handle doesn't stop the sound. You must call `stop` explicitely.
pub struct Handle<'a> {
    engine: &'a Engine,
    decoder_id: usize,
    total_duration_ms: u32,
    remaining_duration_ms: Arc<AtomicUsize>,
}

impl<'a> Handle<'a> {
    #[inline]
    pub fn set_volume(&self, value: f32) {
        let commands = self.engine.commands.lock().unwrap();
        commands.send(Command::SetVolume(self.decoder_id, value)).unwrap();
    }

    #[inline]
    pub fn stop(self) {
        let commands = self.engine.commands.lock().unwrap();
        commands.send(Command::Stop(self.decoder_id)).unwrap();

        if let Some(ref thread) = self.engine.thread {
            thread.unpark();
        }
    }

    #[inline]
    pub fn get_total_duration_ms(&self) -> u32 {
        self.total_duration_ms
    }

    #[inline]
    pub fn get_remaining_duration_ms(&self) -> u32 {
        self.remaining_duration_ms.load(Ordering::Relaxed) as u32
    }
}

pub enum Command {
    Play(Endpoint, Option<Voice>, Box<Decoder<Item=f32> + Send>, Arc<AtomicUsize>),
    Stop(usize),
    SetVolume(usize, f32),
}

fn background(rx: Receiver<Command>) {
    // for each endpoint name, stores the voice and the list of sounds with their volume
    let mut voices: HashMap<String, (Voice, Vec<(Box<Decoder<Item=f32> + Send>, Arc<AtomicUsize>, f32)>)> = HashMap::new();

    // list of sounds to stop playing
    let mut sounds_to_remove: Vec<*const (Decoder<Item=f32> + Send)> = Vec::new();

    // stores the time when the next loop must start
    let mut next_loop_timer = time::precise_time_ns();

    loop {
        // sleeping so that we get a loop every `FIXED_STEP_MS` millisecond
        {
            let now = time::precise_time_ns();
            if next_loop_timer > now + 1000000 {
                let sleep = next_loop_timer - now;
                thread::park_timeout_ms((sleep / 1000000) as u32);
            }
            next_loop_timer += FIXED_STEP_NS;
        }

        // polling for new commands
        if let Ok(command) = rx.try_recv() {
            match command {
                Command::Play(endpoint, mut new_voice, decoder, remaining_duration_ms) => {
                    if let Some(ref mut new_voice) = new_voice {
                        // we initialize the new voice by writing one period of 0s,
                        // so that we are always one period ahead of time
                        let period = new_voice.get_period();
                        let mut buffer = new_voice.append_data(period);

                        match buffer {
                            UnknownTypeBuffer::U16(ref mut buffer) => {
                                for o in buffer.iter_mut() { *o = 32768; }
                            },
                            UnknownTypeBuffer::I16(ref mut buffer) => {
                                for o in buffer.iter_mut() { *o = 0; }
                            },
                            UnknownTypeBuffer::F32(ref mut buffer) => {
                                for o in buffer.iter_mut() { *o = 0.0; }
                            },
                        }
                    }

                    let mut entry = voices.entry(endpoint.get_name()).or_insert_with(|| {
                        (new_voice.unwrap(), Vec::new())
                    });

                    entry.1.push((decoder, remaining_duration_ms, 1.0));
                },

                Command::Stop(decoder) => {
                    for (_, &mut (_, ref mut sounds)) in voices.iter_mut() {
                        sounds.retain(|dec| {
                            &*dec.0 as *const _ as *const u8 as usize != decoder
                        })
                    }
                },

                Command::SetVolume(decoder, volume) => {
                    for (_, &mut (_, ref mut sounds)) in voices.iter_mut() {
                        if let Some(d) = sounds.iter_mut()
                                               .find(|dec| &*dec.0 as *const _ as *const u8 as usize == decoder)
                        {
                            d.2 = volume;
                        }
                    }
                },
            }
        }

        // removing sounds that have finished playing
        for decoder in mem::replace(&mut sounds_to_remove, Vec::new()) {
            for (_, &mut (_, ref mut sounds)) in voices.iter_mut() {
                sounds.retain(|dec| &*dec.0 as *const _ != decoder);
            }
        }

        // updating the existing sounds
        for (_, &mut (ref mut voice, ref mut sounds)) in voices.iter_mut() {
            // writing to the output
            {
                // building an iterator that produces samples from `sounds`
                let num_sounds = sounds.len() as f32;
                let samples_iter = (0..).map(|_| {
                    sounds.iter_mut().map(|s| s.0.next().unwrap_or(0.0) * s.2 / num_sounds)
                          .fold(0.0, |a, b| a + b)
                });

                let mut buffer = {
                    let samples_to_write = voice.get_samples_rate().0 * voice.get_channels() as u32 * FIXED_STEP_MS / 1000;
                    let samples_to_write = cmp::max(samples_to_write as usize, voice.get_period());
                    voice.append_data(samples_to_write)
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

            // updating the contents of `remaining_duration_ms`
            for &(ref decoder, ref remaining_duration_ms, _) in sounds.iter() {
                let (num_samples, _) = decoder.size_hint();
                let num_samples = num_samples + voice.get_pending_samples();
                let value = (num_samples as u64 * 1000 / (voice.get_channels() as u64 *
                                                        voice.get_samples_rate().0 as u64)) as u32;
                remaining_duration_ms.store(value as usize, Ordering::Relaxed);
            }

            // TODO: do better
            voice.play();
        }
    }
}
