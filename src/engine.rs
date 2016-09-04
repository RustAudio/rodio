use std::cmp;
use std::mem;
use std::collections::HashMap;
use std::thread::{self, Builder, Thread};
use std::time::Duration;
use std::sync::mpsc::{self, Sender, Receiver};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;

use cpal;
use cpal::UnknownTypeBuffer;
use cpal::Voice;
use cpal::Endpoint;
use conversions::Sample;

use source::Source;
use source::UniformSourceIterator;

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
    ///
    /// The key of the hashmap is the name of the endpoint, and the value are the channels count
    /// and samples rate.
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
            voices_formats: Mutex::new(HashMap::with_capacity(1)),
        }
    }

    /// Builds a new sink that targets a given endpoint.
    pub fn start(&self, endpoint: &Endpoint) -> Handle {
        // try looking for an existing `Voice`, or create one if there isn't one
        // `new_voice` is `Some` if a new voice has been created
        let (new_voice, channels_count, samples_rate) = {
            let mut voices_formats = self.voices_formats.lock().unwrap();

            // will contain the new voice, or `None` if no new voice is needed
            let mut new_voice = None;

            let &mut (c, s) = voices_formats.entry(endpoint.get_name()).or_insert_with(|| {
                // TODO: handle possible errors here
                // determining the format to use for the new voice
                let format = endpoint.get_supported_formats_list().unwrap().fold(None, |f1, f2| {
                    if f1.is_none() {
                        return Some(f2);
                    }

                    let f1 = f1.unwrap();

                    // we privilege f32 formats to avoid a conversion
                    if f2.data_type == cpal::SampleFormat::F32 && f1.data_type != cpal::SampleFormat::F32 {
                        return Some(f2);
                    }

                    // do not go below 44100 if possible
                    if f1.samples_rate.0 < 44100 {
                        return Some(f2);
                    }

                    // priviledge outputs with 2 channels for now
                    if f2.channels.len() == 2 && f1.channels.len() != 2 {
                        return Some(f2);
                    }

                    Some(f1)
                }).expect("The endpoint doesn't support any format!?");

                new_voice = Some(Voice::new(&endpoint, &format).unwrap());
                (format.channels.len() as u16, format.samples_rate.0)
            });

            (new_voice, c, s)
        };

        // `next_sounds` contains a Vec that can later be used to append new iterators to the sink
        let next_sounds = Arc::new(Mutex::new(Vec::new()));

        // the `QueueIterator` is the main source of samples and will be read by the background
        // thread
        let source = QueueIterator {
            current: Box::new(None.into_iter()),
            next: next_sounds.clone(),
        };

        // we use the pointer of the `Arc` of `next_sounds` as an identifier for this sink for the
        // purpose of communicating with the background thread
        let source_id = &*next_sounds as *const Mutex<_> as *const u8 as usize;

        // at each loop, the background thread will store the remaining time of the sound in this
        // value
        // the initial value is `0` since there's no sound
        let remaining_duration_ms = Arc::new(AtomicUsize::new(0 as usize));

        // send the play command, passing everything to the background thread
        {
            let command = Command::Play(endpoint.clone(), new_voice, source,
                                        remaining_duration_ms.clone());
            self.commands.lock().unwrap().send(command).unwrap();
        }

        Handle {
            engine: self,
            source_id: source_id,
            remaining_duration_ms: remaining_duration_ms,
            samples_rate: samples_rate,
            channels: channels_count,
            next_sounds: next_sounds,
        }
    }
}

/// A sink.
///
/// Note that dropping the handle doesn't delete the sink. You must call `stop` explicitely.
pub struct Handle<'a> {
    engine: &'a Engine,
    source_id: usize,
    remaining_duration_ms: Arc<AtomicUsize>,

    samples_rate: u32,
    channels: u16,

    // Holds a pointer to the list of iterators to be played after the current one has
    // finished playing.
    next_sounds: Arc<Mutex<Vec<Box<Iterator<Item = f32> + Send>>>>,
}

impl<'a> Handle<'a> {
    /// Appends a new source of data after the current one.
    #[inline]
    pub fn append<S>(&self, source: S)
        where S: Source + Send + 'static, S::Item: Sample + Clone + Send
    {
        // adding the estimated duration of the sound to `remaining_duration_ms`
        if let Some(duration) = source.get_total_duration() {
            let duration = duration.as_secs() as usize * 1000 +
                           duration.subsec_nanos() as usize / 1000000;
            self.remaining_duration_ms.fetch_add(duration, Ordering::Relaxed);

        } else {
            let duration = source.size_hint().0 * 1000 / (source.get_samples_rate() as usize *
                                                          source.get_channels() as usize);
            self.remaining_duration_ms.fetch_add(duration, Ordering::Relaxed);
        }

        // pushing the source to `next_sounds`
        let source = UniformSourceIterator::new(source, self.channels, self.samples_rate);
        let source = Box::new(source);
        self.next_sounds.lock().unwrap().push(source);
    }

    /// Changes the volume of the sound played by this sink.
    #[inline]
    pub fn set_volume(&self, value: f32) {
        let commands = self.engine.commands.lock().unwrap();
        commands.send(Command::SetVolume(self.source_id, value)).unwrap();
    }

    /// Stops the sound.
    // note that this method could take `self` instead of `&self`, but it makes the `Sink` object's
    // life easier not to take `self`
    #[inline]
    pub fn stop(&self) {
        let commands = self.engine.commands.lock().unwrap();
        commands.send(Command::Stop(self.source_id)).unwrap();

        if let Some(ref thread) = self.engine.thread {
            thread.unpark();
        }
    }

    /// Returns the minimum estimated duration of the sound being played by this sink.
    #[inline]
    pub fn get_min_remaining_duration(&self) -> Duration {
        Duration::from_millis(self.remaining_duration_ms.load(Ordering::Relaxed) as u64)
    }
}

/// A command sent by the regular threads to the background thread.
pub enum Command {
    /// Adds a new voice to the list of voices to process.
    Play(Endpoint, Option<Voice>, QueueIterator, Arc<AtomicUsize>),

    /// Stops a voice.
    Stop(usize),

    /// Changes the volume of a voice.
    SetVolume(usize, f32),
}

fn background(rx: Receiver<Command>) {
    // for each endpoint name, stores the voice and the list of sounds with their volume
    let mut voices: HashMap<String, (Voice, Vec<(QueueIterator, Arc<AtomicUsize>, f32)>)> = HashMap::new();

    // list of sounds to stop playing
    let mut sounds_to_remove: Vec<*const Mutex<Vec<Box<Iterator<Item = f32> + Send>>>> = Vec::new();

    // stores the time when the next loop must start
    let mut next_loop_timer = time::precise_time_ns();

    loop {
        // sleeping so that we get a loop every `FIXED_STEP_MS` millisecond
        {
            let now = time::precise_time_ns();
            if next_loop_timer > now + 1000000 /* 1ms */ {
                let sleep = next_loop_timer - now;
                thread::park_timeout(Duration::from_millis(sleep / 1000000));
            }
            next_loop_timer += FIXED_STEP_NS;
        }

        // polling for new commands
        if let Ok(command) = rx.try_recv() {
            match command {
                Command::Play(endpoint, new_voice, decoder, remaining_duration_ms) => {
                    let mut entry = voices.entry(endpoint.get_name()).or_insert_with(|| {
                        (new_voice.unwrap(), Vec::new())
                    });

                    entry.1.push((decoder, remaining_duration_ms, 1.0));
                },

                Command::Stop(decoder) => {
                    for (_, &mut (_, ref mut sounds)) in voices.iter_mut() {
                        sounds.retain(|dec| {
                            &*dec.0.next as *const Mutex<_> as *const u8 as usize != decoder
                        })
                    }
                },

                Command::SetVolume(decoder, volume) => {
                    for (_, &mut (_, ref mut sounds)) in voices.iter_mut() {
                        if let Some(d) = sounds.iter_mut()
                                               .find(|dec| &*dec.0.next as *const Mutex<_> as *const u8 as usize == decoder)
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
                sounds.retain(|dec| &*dec.0.next as *const Mutex<_> != decoder);
            }
        }

        // updating the existing sounds
        for (_, &mut (ref mut voice, ref mut sounds)) in voices.iter_mut() {
            // we want the number of samples remaining to be processed by the sound to be around
            // twice the number of samples that are being processed in one loop, with a minimum of 2 periods
            let samples_read_per_loop = (voice.get_samples_rate().0 * voice.get_channels() as u32 * FIXED_STEP_MS / 1000) as usize;
            let pending_samples = voice.get_pending_samples();
            let period = cmp::max(voice.get_period(), 1);
            let samples_required_in_buffer = cmp::max(samples_read_per_loop * 2, period * 2);

            // writing to the output
            if pending_samples < samples_required_in_buffer {
                // building an iterator that produces samples from `sounds`
                let samples_iter = (0..).map(|_| {
                    sounds.iter_mut().map(|s| s.0.next().unwrap_or(0.0) * s.2)
                          .fold(0.0, |a, b| { let v = a + b; if v > 1.0 { 1.0 } else if v < -1.0 { -1.0 } else { v } })
                });

                let mut buffer = voice.append_data(samples_required_in_buffer - pending_samples);

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
                // TODO: differenciate sounds from this sink from sounds from other sinks
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

/// Main source of samples for a voice.
pub struct QueueIterator {
    /// The current iterator that produces samples.
    current: Box<Iterator<Item = f32> + Send>,

    /// A `Vec` containing the next iterators to play. Shared with other threads so they can add
    /// sounds to the list.
    next: Arc<Mutex<Vec<Box<Iterator<Item = f32> + Send>>>>,
}

impl Iterator for QueueIterator {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        loop {
            // basic situation that will happen most of the time
            if let Some(sample) = self.current.next() {
                return Some(sample);
            }

            let next = {
                let mut next = self.next.lock().unwrap();
                if next.len() == 0 {
                    // if there's no iter waiting, we create a dummy iter with 1000 null samples
                    // this avoids a spinlock
                    Box::new((0 .. 1000).map(|_| 0.0f32)) as Box<Iterator<Item = f32> + Send>
                } else {
                    next.remove(0)
                }
            };

            self.current = next;
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        // TODO: slow? benchmark this
        let next_hints = self.next.lock().unwrap().iter()
                                  .map(|i| i.size_hint().0).fold(0, |a, b| a + b);
        (self.current.size_hint().0 + next_hints, None)
    }
}
