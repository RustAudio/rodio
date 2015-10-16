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
use std::thread;

mod conversions;
mod engine;

pub mod decoder;
pub mod source;

lazy_static! {
    static ref ENGINE: engine::Engine = engine::Engine::new();
}

/// Handle to a playing sound.
///
/// Note that dropping the handle doesn't stop the sound. You must call `stop` explicitely.
pub struct Sink(engine::Handle<'static>);

impl Sink {
    pub fn new(endpoint: &Endpoint) -> Sink {
        Sink(ENGINE.start(&endpoint))
    }

    pub fn append<S>(&self, source: S) where S: Source + Send + 'static,
                                             S::Item: Sample, S::Item: Send
    {
        self.0.append(source);
    }

    /// Changes the volume of the sound.
    ///
    /// The value `1.0` is the "normal" volume (unfiltered input). Any value other than 1.0 will
    /// multiply each sample by this value.
    #[inline]
    pub fn set_volume(&mut self, value: f32) {
        self.0.set_volume(value);
    }

    /// Stops the sound.
    #[inline]
    pub fn stop(self) {
        self.0.stop()
    }

    /// Returns the minimum number of milliseconds remaining before the end of the sound.
    ///
    /// Note that this is a minimum value, and the sound can last longer.
    #[inline]
    pub fn get_remaining_duration_ms(&self) -> u32 {
        self.0.get_remaining_duration_ms()
    }

    /// Sleeps the current thread until the sound ends.
    #[inline]
    pub fn sleep_until_end(&self) {
        // TODO: sleep repeatidely until the sound is finished (see the docs of `get_remaining_duration`)
        thread::sleep_ms(self.get_remaining_duration_ms());
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
