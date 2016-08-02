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

use futures::Future;
use futures::stream::Stream;

use cpal;
use cpal::Format;
use cpal::UnknownTypeBuffer;
use cpal::EventLoop;
use cpal::Voice;
use cpal::SamplesStream;
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
    /// The events loop which the voices are created with.
    events_loop: Arc<EventLoop>,

    end_points: Mutex<HashMap<String, Arc<EndPointVoices>>>,        // TODO: don't use the endpoint name, as it's slow
}

struct EndPointVoices {
    format: Format,
    next_id: AtomicUsize,
    sounds: Mutex<HashMap<usize, QueueIterator>>,       // TODO: fnv hasher
}

impl Engine {
    /// Builds the engine.
    pub fn new() -> Engine {
        let events_loop = Arc::new(EventLoop::new());

        // we ignore errors when creating the background thread
        // the user won't get any audio, but that's better than a panic
        let thread = {
            let events_loop = events_loop.clone();
            Builder::new().name("rodio audio processing".to_string())
                          .spawn(move || events_loop.run())
                          .ok().map(|jg| jg.thread().clone())
        };

        Engine {
            events_loop: events_loop,
            end_points: Mutex::new(HashMap::with_capacity(1)),
        }
    }

    /// Builds a new sink that targets a given endpoint.
    pub fn start(&self, endpoint: &Endpoint) -> Handle {
        // Getting the `EndPointVoices` struct of the requested endpoint.
        let end_point = self.end_points.lock().unwrap().entry(endpoint.get_name()).or_insert_with(|| {
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

            let (mut voice, stream) = Voice::new(&endpoint, &format, &self.events_loop).unwrap();
            let end_point_voices = Arc::new(EndPointVoices {
                format: format,
                next_id: AtomicUsize::new(1),
                sounds: Mutex::new(HashMap::with_capacity(8)),
            });

            let epv = end_point_voices.clone();
            stream.for_each(move |mut buffer| -> Result<_, ()> {
                let mut sounds = epv.sounds.lock().unwrap();

                let samples_iter = (0..).map(|_| {
                    sounds.values_mut().map(|s| s.next().unwrap_or(0.0) /* TODO: multiply by volume */)
                          .fold(0.0, |a, b| { let v = a + b; if v > 1.0 { 1.0 } else if v < -1.0 { -1.0 } else { v } })
                });

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
                };

                Ok(())
            }).forget();

            voice.play();

            end_point_voices
        }).clone();

        // Assigning an id for the handle.
        let handle_id = end_point.next_id.fetch_add(1, Ordering::Relaxed);

        // `next_sounds` contains a Vec that can later be used to append new iterators to the sink
        let next_sounds = Arc::new(Mutex::new(Vec::new()));
        let queue_iterator = QueueIterator {
            current: Box::new(None.into_iter()),
            signal_after_end: None,
            next: next_sounds.clone(),
        };

        // Adding the new sound to the list of parallel sounds.
        end_point.sounds.lock().unwrap().insert(handle_id, queue_iterator);

        // Returning the handle.
        Handle {
            handle_id: handle_id,
            samples_rate: end_point.format.samples_rate.0,
            channels: end_point.format.channels.len() as u16,
            next_sounds: next_sounds,
            end: Mutex::new(None),
        }
    }
}

/// A sink.
///
/// Note that dropping the handle doesn't delete the sink. You must call `stop` explicitely.
pub struct Handle {
    handle_id: usize,

    samples_rate: u32,
    channels: u16,

    // Holds a pointer to the list of iterators to be played after the current one has
    // finished playing.
    next_sounds: Arc<Mutex<Vec<(Box<Iterator<Item = f32> + Send>, Option<Sender<()>>)>>>,

    // Receiver that is triggered when the last sound ends.
    end: Mutex<Option<Receiver<()>>>,
}

impl Handle {
    /// Appends a new source of data after the current one.
    ///
    /// Returns a receiver that is triggered when the sound is finished playing.
    #[inline]
    pub fn append<S>(&self, source: S)
        where S: Source + Send + 'static, S::Item: Sample + Clone + Send
    {
        // Updating `end`.
        let (tx, rx) = mpsc::channel();
        *self.end.lock().unwrap() = Some(rx);

        // Pushing the source and the `tx` to `next_sounds`.
        let source = UniformSourceIterator::new(source, self.channels, self.samples_rate);
        let source = Box::new(source);
        self.next_sounds.lock().unwrap().push((source, Some(tx)));
    }

    /// Changes the volume of the sound played by this sink.
    #[inline]
    pub fn set_volume(&self, value: f32) {
        // FIXME:
    }

    /// Stops the sound.
    // note that this method could take `self` instead of `&self`, but it makes the `Sink` object's
    // life easier not to take `self`
    #[inline]
    pub fn stop(&self) {
        // FIXME:
    }

    /// Sleeps the current thread until the sound ends.
    #[inline]
    pub fn sleep_until_end(&self) {
        // Will either block when reading `end`, or will block in the mutex lock if another
        // thread is already reading `end`.
        let mut end = self.end.lock().unwrap();
        if let Some(end) = end.take() {
            let _ = end.recv();
        }
    }
}

// Main source of samples for a voice.
struct QueueIterator {
    // The current iterator that produces samples.
    current: Box<Iterator<Item = f32> + Send>,

    // Signal this sender before picking from `next`.
    signal_after_end: Option<Sender<()>>,

    // A `Vec` containing the next iterators to play. Shared with other threads so they can add
    // sounds to the list.
    next: Arc<Mutex<Vec<(Box<Iterator<Item = f32> + Send>, Option<Sender<()>>)>>>,
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

            if let Some(signal_after_end) = self.signal_after_end.take() {
                let _ = signal_after_end.send(());
            }

            let (next, signal_after_end) = {
                let mut next = self.next.lock().unwrap();
                if next.len() == 0 {
                    // if there's no iter waiting, we create a dummy iter with 1000 null samples
                    // this avoids a spinlock
                    (Box::new((0 .. 1000).map(|_| 0.0f32)) as Box<Iterator<Item = f32> + Send>, None)
                } else {
                    next.remove(0)
                }
            };

            self.current = next;
            self.signal_after_end = signal_after_end;
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        // TODO: slow? benchmark this
        let next_hints = self.next.lock().unwrap().iter()
                                  .map(|i| i.0.size_hint().0).fold(0, |a, b| a + b);
        (self.current.size_hint().0 + next_hints, None)
    }
}
