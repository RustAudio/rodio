extern crate arrayvec;
extern crate cpal;
extern crate hound;
#[macro_use]
extern crate lazy_static;
extern crate time;
extern crate vorbis;

pub use cpal::{Endpoint, get_endpoints_list, get_default_endpoint};

use std::io::{Read, Seek};

mod conversions;
mod decoder;
mod engine;

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
}

/// Plays a sound once. Returns a `Handle` that can be used to control the sound.
#[inline]
pub fn play_once<R>(endpoint: &Endpoint, input: R) -> Handle
                    where R: Read + Seek + Send + 'static
{
    Handle(ENGINE.play(&endpoint, input))
}
