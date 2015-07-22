extern crate cpal;
extern crate hound;
#[macro_use]
extern crate lazy_static;

use std::io::Read;

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
    pub fn stop(self) {
        self.0.stop()
    }
}

/// Plays a sound once. There's no way to stop the sound except by exiting the program.
pub fn play_once<R>(input: R) -> Handle where R: Read + Send + 'static {
    let decoder = decoder::decode(input);
    Handle(ENGINE.play(decoder))
}
