//! Audio playback library.
//!
//! The main concept of this library is [the `Source` trait](source/trait.Source.html), which
//! represents a sound (streaming or not). In order to play a sound, there are three steps:
//!
//! - Create an object that represents the streaming sound. It can be a sine wave, a buffer, a
//!   [decoder](decoder/index.html), etc. or even your own type that implements
//!   [the `Source` trait](source/trait.Source.html).
//! - Choose an output with the [`devices`](fn.devices.html) or
//!   [`default_output_device`](fn.default_output_device.html) functions.
//! - Call [`play_raw(output, source)`](fn.play_raw.html).
//!
//! The `play_raw` function expects the source to produce `f32`s, which may not be the case. If you
//! get a compilation error, try calling `.convert_samples()` on the source to fix it.
//!
//! For example, here is how you would play an audio file:
//!
//! ```no_run
//! use std::fs::File;
//! use std::io::BufReader;
//! use rodio::Source;
//!
//! let device = rodio::default_output_device().unwrap();
//!
//! let file = File::open("sound.ogg").unwrap();
//! let source = rodio::Decoder::new(BufReader::new(file)).unwrap();
//! rodio::play_raw(&device, source.convert_samples());
//! ```
//!
//! ## Sink
//!
//! In order to make it easier to control the playback, the rodio library also provides a type
//! named [`Sink`](struct.Sink.html) which represents an audio track.
//!
//! Instead of playing the sound with [`play_raw`](fn.play_raw.html), you can add it to a
//! [`Sink`](struct.Sink.html) instead.
//!
//! ```no_run
//! use rodio::Sink;
//!
//! let device = rodio::default_output_device().unwrap();
//! let sink = Sink::new(&device);
//!
//! // Add a dummy source of the sake of the example.
//! let source = rodio::source::SineWave::new(440);
//! sink.append(source);
//! ```
//!
//! The [`append` method](struct.Sink.html#method.append) will add the sound at the end of the
//! sink. It will be played when all the previous sounds have been played. If you want multiple
//! sounds to play simultaneously, you should create multiple [`Sink`](struct.Sink.html)s.
//!
//! The [`Sink`](struct.Sink.html) type also provides utilities such as playing/pausing or
//! controlling the volume.
//!
//! ## Filters
//!
//! [The `Source` trait](source/trait.Source.html) provides various filters, similarly to the
//! standard `Iterator` trait.
//!
//! Example:
//!
//! ```
//! use rodio::Source;
//! use std::time::Duration;
//!
//! // Repeats the first five seconds of the sound forever.
//! # let source = rodio::source::SineWave::new(440);
//! let source = source.take_duration(Duration::from_secs(5)).repeat_infinite();
//! ```
//!
//! ## How it works under the hood
//!
//! Rodio spawns a background thread that is dedicated to reading from the sources and sending
//! the output to the device. Whenever you give up ownership of a `Source` in order to play it,
//! it is sent to this background thread where it will be read by rodio.
//!
//! All the sounds are mixed together by rodio before being sent to the operating system or the
//! hardware. Therefore there is no restriction on the number of sounds that play simultaneously or
//! the number of sinks that can be created (except for the fact that creating too many will slow
//! down your program).
//!

#![cfg_attr(test, deny(missing_docs))]

#[cfg(feature = "flac")]
extern crate claxon;
extern crate cpal;
#[cfg(feature = "wav")]
extern crate hound;
#[macro_use]
extern crate lazy_static;
#[cfg(feature = "vorbis")]
extern crate lewton;
#[cfg(feature = "mp3")]
extern crate minimp3;
#[cfg(feature = "http")]
extern crate reqwest;

pub use cpal::{
    traits::DeviceTrait, Device, Devices, DevicesError, Format, InputDevices, OutputDevices
};

pub use conversions::Sample;
pub use decoder::Decoder;
pub use engine::play_raw;
pub use sink::Sink;
pub use source::Source;
pub use spatial_sink::SpatialSink;
#[cfg(feature = "http")]
pub use utils::{
    buffer::seekable_bufreader::SeekableBufReader,
    source::http::SeekableRequest,
};

use cpal::traits::HostTrait;
use std::io::{Read, Seek};

mod conversions;
mod engine;
mod sink;
mod spatial_sink;

pub mod buffer;
pub mod decoder;
pub mod dynamic_mixer;
pub mod utils;
pub mod queue;
pub mod source;
pub mod static_buffer;

/// Plays a sound once. Returns a `Sink` that can be used to control the sound.
#[inline]
pub fn play_once<R>(device: &Device, input: R) -> Result<Sink, decoder::DecoderError>
where
    R: Read + Seek + Send + 'static,
{
    let input = decoder::Decoder::new(input)?;
    let sink = Sink::new(device);
    sink.append(input);
    Ok(sink)
}

/// The default input audio device on the system.
///
/// Returns `None` if no input device is available.
#[inline]
pub fn default_input_device() -> Option<Device> {
    cpal::default_host().default_input_device()
}

/// The default output audio device on the system.
///
/// Returns `None` if no output device is available.
#[inline]
pub fn default_output_device() -> Option<Device> {
    cpal::default_host().default_output_device()
}

/// An iterator yielding all `Device`s currently available to the host on the system.
///
/// Can be empty if the system does not support audio in general.
#[inline]
pub fn devices() -> Result<Devices, DevicesError> {
    cpal::default_host().devices()
}

/// An iterator yielding all `Device`s currently available to the system that support one or more
/// input stream formats.
///
/// Can be empty if the system does not support audio input.
#[inline]
pub fn input_devices() -> Result<InputDevices<Devices>, DevicesError> {
    cpal::default_host().input_devices()
}

/// An iterator yielding all `Device`s currently available to the system that support one or more
/// output stream formats.
///
/// Can be empty if the system does not support audio output.
#[inline]
pub fn output_devices() -> Result<OutputDevices<Devices>, DevicesError> {
    cpal::default_host().output_devices()
}
