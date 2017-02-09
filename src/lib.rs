//! # Usage
//!
//! There are two main concepts in this library:
//!
//! - Sources, represented with [the `Source` trait](source/trait.Source.html), that provide sound
//!   data.
//! - Sinks, which accept sound data.
//!
//! > **Note**: If you are not familiar with what a sound is or how a sound is stored in memory,
//! > check out the documentation of [the `Source` trait](source/trait.Source.html).
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

extern crate claxon;
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
pub use engine::play_raw;
pub use sink::Sink;
pub use source::Source;

use std::io::{Read, Seek};

mod conversions;
mod engine;
mod sink;

pub mod decoder;
pub mod dynamic_mixer;
pub mod queue;
pub mod source;

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
