#![cfg_attr(test, deny(missing_docs))]
#![cfg_attr(test, deny(warnings))]

extern crate cpal;
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
    handle: engine::Handle<'static>,
    // if true, then the sound will stop playing at the end
    stop: bool,
}

impl Sink {
    /// Builds a new `Sink`.
    pub fn new(endpoint: &Endpoint) -> Sink {
        Sink {
            handle: ENGINE.start(&endpoint),
            stop: true,
        }
    }

    /// Appends a sound to the queue of sounds to play.
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

    /// Returns the minimum duration before the end of the sounds submitted to this sink.
    ///
    /// Note that this is a minimum value, and the sound can last longer.
    #[inline]
    pub fn get_min_remaining_duration(&self) -> Duration {
        self.handle.get_min_remaining_duration()
    }

    /// Sleeps the current thread until the sound ends.
    #[inline]
    pub fn sleep_until_end(&self) {
        // TODO: sleep repeatidely until the sound is finished (see the docs of `get_remaining_duration`)
        thread::sleep(self.get_min_remaining_duration());
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
pub fn play_once<R>(endpoint: &Endpoint, input: R) -> Sink
                    where R: Read + Seek + Send + 'static
{
    let input = decoder::Decoder::new(input);
    play(endpoint, input)
}

/// Plays a sound.
pub fn play<S>(endpoint: &Endpoint, source: S) -> Sink where S: Source + Send + 'static,
                                                             S::Item: Sample, S::Item: Send
{
    let sink = Sink::new(endpoint);
    sink.append(source);
    sink
}
