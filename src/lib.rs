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
extern crate time;
extern crate vorbis;

pub use cpal::{Endpoint, get_endpoints_list, get_default_endpoint};

pub use conversions::Sample;
pub use decoder::Decoder;
pub use source::Source;

use std::io::{Read, Seek};
use std::time::Duration;
use std::thread;

mod conversions;
mod engine;

pub mod decoder;
pub mod source;

lazy_static! {
    static ref ENGINE: engine::Engine = engine::Engine::new();
}

/// Handle to an endpoint that outputs sounds.
///
/// Dropping the `Sink` stops all sounds. You can use `detach` if you want the sounds to continue
/// playing.
pub struct Sink {
    handle: engine::Handle,
    // if true, then the sound will stop playing at the end
    stop: bool,
}

impl Sink {
    /// Builds a new `Sink`.
    #[inline]
    pub fn new(endpoint: &Endpoint) -> Sink {
        Sink {
            handle: ENGINE.start(&endpoint),
            stop: true,
        }
    }

    /// Appends a sound to the queue of sounds to play.
    #[inline]
    pub fn append<S>(&self, source: S) where S: Source + Send + 'static,
                                             S::Item: Sample, S::Item: Send
    {
        self.handle.append(source);
    }

    /// Changes the volume of the sound.
    ///
    /// The value `1.0` is the "normal" volume (unfiltered input). Any value other than 1.0 will
    /// multiply each sample by this value.
    #[inline]
    pub fn set_volume(&mut self, value: f32) {
        self.handle.set_volume(value);
    }

    /// Destroys the sink without stopping the sounds that are still playing.
    #[inline]
    pub fn detach(mut self) {
        self.stop = false;
    }

    /// Sleeps the current thread until the sound ends.
    #[inline]
    pub fn sleep_until_end(&self) {
        self.handle.sleep_until_end();
    }
}

impl Drop for Sink {
    #[inline]
    fn drop(&mut self) {
        if self.stop {
            self.handle.stop();
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
