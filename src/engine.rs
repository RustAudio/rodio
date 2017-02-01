use std::collections::HashMap;
use std::thread::Builder;
use std::sync::mpsc::{self, Sender, Receiver};
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use futures::stream::Stream;
use futures::task;
use futures::task::Executor;
use futures::task::Run;

use cpal;
use cpal::Format;
use cpal::UnknownTypeBuffer;
use cpal::EventLoop;
use cpal::Voice;
use cpal::Endpoint;
use conversions::Sample;

use source::Source;
use source::UniformSourceIterator;

use engine_filters::Pauseable;
use engine_filters::VolumeFilter;

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
    pending_sounds: Mutex<Vec<(usize, QueueIterator)>>,
}

impl Engine {
    /// Builds the engine.
    pub fn new() -> Engine {
        let events_loop = Arc::new(EventLoop::new());

        // We ignore errors when creating the background thread.
        // The user won't get any audio, but that's better than a panic.
        Builder::new()
            .name("rodio audio processing".to_string())
            .spawn({
                let events_loop = events_loop.clone();
                move || events_loop.run()
            })
            .ok().map(|jg| jg.thread().clone());

        Engine {
            events_loop: events_loop,
            end_points: Mutex::new(HashMap::with_capacity(1)),
        }
    }

    /// Builds a new sink that targets a given endpoint.
    pub fn start(&self, endpoint: &Endpoint) -> Handle {
        let mut future_to_exec = None;

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
                pending_sounds: Mutex::new(Vec::with_capacity(8)),
            });

            let epv = end_point_voices.clone();

            let sounds = Arc::new(Mutex::new(Vec::new()));
            future_to_exec = Some(stream.for_each(move |mut buffer| -> Result<_, ()> {
                let mut sounds = sounds.lock().unwrap();

                {
                    let mut pending = epv.pending_sounds.lock().unwrap();
                    sounds.append(&mut pending);
                }

                if sounds.len() == 0 {
                    return Ok(());
                }
                // Drop if it's not playing a real source, and it's sink is detached
                // or the sink was dropped before being detached.
                sounds.retain(|s| {
                    if s.1.local_dead {
                        return false;
                    }
                    if !s.1.is_playing_real_source {
                        return !s.1.local_handle_dead;
                    } else {
                        true
                    }
                });
                let samples_iter = (0..).map(|_| {
                    let v = sounds.iter_mut().map(|s| s.1.next().unwrap_or(0.0))
                                  .fold(0.0, |a, b| a + b);
                    if v < -1.0 { -1.0 } else if v > 1.0 { 1.0 } else { v }
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
            }));

            voice.play();       // TODO: don't do this now

            end_point_voices
        }).clone();

        // Assigning an id for the handle.
        let handle_id = end_point.next_id.fetch_add(1, Ordering::Relaxed);

        // Initialize the volume
        let volume = Arc::new(Mutex::new(1.0));

        // If paused is set to true then don't play from this handle.
        let paused = Arc::new(AtomicBool::new(false));

        // If dead is set to true then this Handle should be removed.
        let dead = Arc::new(AtomicBool::new(false));

        // Used to detect detached handles.
        let handle_dead = Arc::new(AtomicBool::new(false));

        // `next_sounds` contains a Vec that can later be used to append new iterators to the sink
        let next_sounds = Arc::new(Mutex::new(Vec::new()));

        // Frequency with which dead value should be updated.
        let update_frequency = (5 * end_point.format.samples_rate.0)/1000;

        let queue_iterator = QueueIterator {
            current: Box::new(None.into_iter()),
            signal_after_end: None,
            next: next_sounds.clone(),
            local_dead: false,
            remote_dead: dead.clone(),
            local_handle_dead: false,
            remote_handle_dead: handle_dead.clone(),
            samples_until_update: update_frequency,
            update_frequency: update_frequency,
            is_playing_real_source: true,
        };

        // Adding the new sound to the list of parallel sounds.
        end_point.pending_sounds.lock().unwrap().push((handle_id, queue_iterator));

        if let Some(future_to_exec) = future_to_exec {
            struct MyExecutor;
            impl Executor for MyExecutor { fn execute(&self, r: Run) { r.run(); } }
            task::spawn(future_to_exec).execute(Arc::new(MyExecutor));
        }

        // Returning the handle.
        Handle {
            handle_id: handle_id,
            samples_rate: end_point.format.samples_rate.0,
            channels: end_point.format.channels.len() as u16,
            next_sounds: next_sounds,
            dead: dead,
            handle_dead: handle_dead,
            paused: paused,
            volume: volume,
            end: Mutex::new(None),
        }
    }
}

/// A sink.
///
/// Note that dropping the handle doesn't delete the sink. You must call `stop` explicitly.
pub struct Handle {
    handle_id: usize,

    samples_rate: u32,
    channels: u16,

    // Pointer to paused value in Pausable
    paused: Arc<AtomicBool>,

    // Pointer to volume value in VolumeFilter
    volume: Arc<Mutex<f32>>,

    // Pointer to dead value in QueueIterator
    dead: Arc<AtomicBool>,

    // Set this to true when we are dropped, regardless of if we were detached or not.
    // This is read by the engine thread.
    handle_dead: Arc<AtomicBool>,

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

        let source = Pauseable::new(source, self.paused.clone(), 5);
        let source = VolumeFilter::new(source, self.volume.clone(), 5);

        // Pushing the source and the `tx` to `next_sounds`.
        let source = UniformSourceIterator::new(source, self.channels, self.samples_rate);
        let source = Box::new(source);
        self.next_sounds.lock().unwrap().push((source, Some(tx)));
    }

    /// Gets the volume of the sound played by this sink.
    #[inline]
    pub fn volume(&self) -> f32 {
        *self.volume.lock().unwrap()
    }

    /// Changes the volume of the sound played by this sink.
    #[inline]
    pub fn set_volume(&self, _value: f32) {
        *self.volume.lock().unwrap() = _value.min(1.0).max(0.0);
    }

    /// If the sound is paused then resume playing it.
    #[inline]
    pub fn play(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }

    /// Pause the sound
    #[inline]
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    /// Returns true if the sound is currently paused
    #[inline]
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Stops the sound.
    // note that this method could take `self` instead of `&self`, but it makes the `Sink` object's
    // life easier not to take `self`
    #[inline]
    pub fn stop(&self) {
        self.dead.store(true, Ordering::Relaxed);
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

impl Drop for Handle {
    #[inline]
    fn drop(&mut self) {
        self.handle_dead.store(true, Ordering::Relaxed);
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

    //Local storage of the dead value.  Allows us to only check the remote occasionally.
    local_dead: bool,

    //The dead value which may be manipulated by another thread.
    remote_dead: Arc<AtomicBool>,

    // Local storage of the handle_dead value.  Allows us to only check the remote occasionally.
    local_handle_dead: bool,

    // Is our handle dead?  Used to identify situations with a dropped handle that's been detached.
    remote_handle_dead: Arc<AtomicBool>,

    //The frequency with which local_dead should be updated by remote_dead
    update_frequency: u32,

    //How many samples remain until it is time to update local_dead with remote_dead.
    samples_until_update: u32,

    // Whether we're playing a source from a sink, or a dummy  iter
    is_playing_real_source: bool,
}

impl Iterator for QueueIterator {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        self.samples_until_update -= 1;
        if self.samples_until_update == 0 {
            self.local_dead = self.remote_dead.load(Ordering::Relaxed);
            self.local_handle_dead = self.remote_handle_dead.load(Ordering::Relaxed);
            self.samples_until_update = self.update_frequency;
        }
        if self.local_dead {
            return Some(0.0);
        }
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
                    self.is_playing_real_source = false;
                    // if there's no iter waiting, we create a dummy iter with 1000 null samples
                    // this avoids a spinlock
                    (Box::new((0 .. 1000).map(|_| 0.0f32)) as Box<Iterator<Item = f32> + Send>, None)
                } else {
                    self.is_playing_real_source = true;
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
