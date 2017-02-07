//! # Usage
//!
//! There are two main concepts in this library:
//!
//! - Sources, represented with the `Source` trait, that provide sound data.
//! - Sinks, which accept sound data.
//!
//! In order to play a sound, you need to create a source, a sink, and connect the two. For example
//! here is how you play a sound file:
//!
//! ```no_run
//! use std::io::BufReader;
//!
//! let endpoint = rodio::get_default_endpoint().unwrap();
//! let sink = rodio::Sink::new(&endpoint);
//!
//! let file = std::fs::File::open("music.ogg").unwrap();
//! let source = rodio::Decoder::new(BufReader::new(file)).unwrap();
//! sink.append(source);
//! ```
//!
//! The `append` method takes ownership of the source and starts playing it. If a sink is already
//! playing a sound when you call `append`, the sound is added to a queue and will start playing
//! when the existing source is over.
//!
//! If you want to play multiple sounds simultaneously, you should create multiple sinks.
//!
//! # How it works
//!
//! Rodio spawns a background thread that is dedicated to reading from the sources and sending
//! the output to the endpoint.
//!
//! All the sounds are mixed together by rodio before being sent. Since this is handled by the
//! software, there is no restriction for the number of sinks that can be created.
//!
//! # Adding effects
//!
//! The `Source` trait provides various filters, similarly to the standard `Iterator` trait.
//!
//! Example:
//!
//! ```ignore
//! use rodio::Source;
//! use std::time::Duration;
//!
//! // repeats the first five seconds of this sound forever
//! let source = source.take_duration(Duration::from_secs(5)).repeat_infinite();
//! ```

#![cfg_attr(test, deny(missing_docs))]

extern crate cpal;
extern crate futures;
extern crate hound;
#[macro_use]
extern crate lazy_static;
extern crate lewton;
extern crate ogg;

pub use cpal::{Endpoint, get_endpoints_list, get_default_endpoint};

pub use conversions::Sample;
pub use decoder::Decoder;
pub use source::Source;

use std::io::{Read, Seek};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::sync::Mutex;

mod conversions;
mod engine;

pub mod decoder;
pub mod dynamic_mixer;
pub mod queue;
pub mod source;

lazy_static! {
    static ref ENGINE: engine::Engine = engine::Engine::new();
}

/// Handle to an endpoint that outputs sounds.
///
/// Dropping the `Sink` stops all sounds. You can use `detach` if you want the sounds to continue
/// playing.
pub struct Sink {
    queue_tx: Arc<queue::SourcesQueueInput<f32>>,
    sleep_until_end: Mutex<Option<Receiver<()>>>,

    pause: Arc<AtomicBool>,
    volume: Arc<Mutex<f32>>,
    stopped: Arc<AtomicBool>,

    detached: bool,
}

impl Sink {
    /// Builds a new `Sink`.
    #[inline]
    pub fn new(endpoint: &Endpoint) -> Sink {
        let (queue_tx, queue_rx) = queue::queue(true);
        ENGINE.start(endpoint, queue_rx);

        Sink {
            queue_tx: queue_tx,
            sleep_until_end: Mutex::new(None),
            pause: Arc::new(AtomicBool::new(false)),
            volume: Arc::new(Mutex::new(1.0)),
            stopped: Arc::new(AtomicBool::new(false)),
            detached: false,
        }
    }

    /// Appends a sound to the queue of sounds to play.
    #[inline]
    pub fn append<S>(&self, source: S)
        where S: Source + Send + 'static,
              S::Item: Sample,
              S::Item: Send
    {
        let source = source::Pauseable::new(source, self.pause.clone(), 5);
        let source = source::Stoppable::new(source, self.stopped.clone(), 5);
        let source = source::VolumeFilter::new(source, self.volume.clone(), 5);
        let source = source::SamplesConverter::new(source);
        *self.sleep_until_end.lock().unwrap() = Some(self.queue_tx.append_with_signal(source));
    }

    // Gets the volume of the sound.
    ///
    /// The value `1.0` is the "normal" volume (unfiltered input). Any value other than 1.0 will
    /// multiply each sample by this value.
    #[inline]
    pub fn volume(&self) -> f32 {
        *self.volume.lock().unwrap()
    }

    /// Changes the volume of the sound.
    ///
    /// The value `1.0` is the "normal" volume (unfiltered input). Any value other than 1.0 will
    /// multiply each sample by this value.
    #[inline]
    pub fn set_volume(&mut self, value: f32) {
        *self.volume.lock().unwrap() = value;
    }

    /// Resumes playback of a paused sound.
    ///
    /// No effect if not paused.
    #[inline]
    pub fn play(&self) {
        self.pause.store(false, Ordering::SeqCst);
    }

    /// Pauses playback of this sink.
    ///
    /// No effect if already paused.
    ///
    /// A paused sound can be resumed with `play()`.
    pub fn pause(&self) {
        self.pause.store(true, Ordering::SeqCst);
    }

    /// Gets if a sound is paused
    ///
    /// Sounds can be paused and resumed using pause() and play(). This gets if a sound is paused.
    pub fn is_paused(&self) -> bool {
        self.pause.load(Ordering::SeqCst)
    }

    /// Destroys the sink without stopping the sounds that are still playing.
    #[inline]
    pub fn detach(mut self) {
        self.detached = true;
    }

    /// Sleeps the current thread until the sound ends.
    #[inline]
    pub fn sleep_until_end(&self) {
        if let Some(sleep_until_end) = self.sleep_until_end.lock().unwrap().take() {
            let _ = sleep_until_end.recv();
        }
    }
}

impl Drop for Sink {
    #[inline]
    fn drop(&mut self) {
        self.queue_tx.set_keep_alive_if_empty(false);

        if !self.detached {
            self.stopped.store(true, Ordering::Relaxed);
        }
    }
}

/// Plays a sound once. Returns a `Sink` that can be used to control the sound.
#[inline]
pub fn play_once<R>(endpoint: &Endpoint, input: R) -> Result<Sink, decoder::DecoderError>
    where R: Read + Seek + Send + 'static
{
    let input = try!(decoder::Decoder::new(input));
    let sink = Sink::new(endpoint);
    sink.append(input);
    Ok(sink)
}
