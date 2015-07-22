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

/// Plays a sound once. There's no way to stop the sound except by exiting the program.
pub fn play_once<R>(input: R) where R: Read + Send + 'static {
    let decoder = decoder::decode(input);
    ENGINE.play_once(decoder);
}
