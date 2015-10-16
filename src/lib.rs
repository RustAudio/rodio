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
pub use source::Source;

use std::io::{Read, Seek};
use std::thread;

mod conversions;
mod decoder;
mod engine;

pub mod source;

lazy_static! {
    static ref ENGINE: engine::Engine = engine::Engine::new();
}

/// Handle to a playing sound.
///
/// Note that dropping the handle doesn't stop the sound. You must call `stop` explicitely.
pub struct Handle(engine::Handle<'static>);

impl Handle {
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

    /// Returns the number of milliseconds in total in the sound file.
    #[inline]
    pub fn get_total_duration_ms(&self) -> u32 {
        self.0.get_total_duration_ms()
    }

    /// Returns the number of milliseconds remaining before the end of the sound.
    #[inline]
    pub fn get_remaining_duration_ms(&self) -> u32 {
        self.0.get_remaining_duration_ms()
    }

    /// Sleeps the current thread until the sound ends.
    #[inline]
    pub fn sleep_until_end(&self) {
        thread::sleep_ms(self.get_remaining_duration_ms());
    }
}

/// Plays a sound once. Returns a `Handle` that can be used to control the sound.
#[inline]
pub fn play_once<R>(endpoint: &Endpoint, input: R) -> Handle
                    where R: Read + Seek + Send + 'static
{
    let input = decoder::Decoder::new(input);
    Handle(ENGINE.play(&endpoint, input))
}

/*pub fn decode<R>() -> DecoderSource where R: Read + Seek + Send + 'static {
    DecoderSource {

    }
}

pub struct Sink {
    handle: engine::Handle<'static>,
}

impl Sink {
    /// Plays a source after the current source (if any) has finished playing.
    #[inline]
    pub fn play<S>(&mut self, source: S) where S: Source {
        let source: Box<Source> = Box::new(source);
        self.handle.add(source);
    }
}*/
