//! Audio playback library.
//!
//! The main concept of this library is the [`Source`] trait, which
//! represents a sound (streaming or not). In order to play a sound, there are three steps:
//!
//! - Create an object that represents the streaming sound. It can be a sine wave, a buffer, a
//!   [`decoder`], etc. or even your own type that implements the [`Source`] trait.
//! - Get an output stream handle to a physical device. For example, get a stream to the system's
//!   default sound device with [`OutputStream::try_default()`]
//! - Call [`.play_raw(source)`](OutputStreamHandle::play_raw) on the output stream handle.
//!
//! The [`play_raw`](OutputStreamHandle::play_raw) function expects the source to produce [`f32`]s,
//! which may not be the case. If you get a compilation error, try calling
//! [`.convert_samples()`](Source::convert_samples) on the source to fix it.
//!
//! For example, here is how you would play an audio file:
//!
//! ```no_run
//! use std::fs::File;
//! use std::io::BufReader;
//! use rodio::{Decoder, OutputStream, source::Source};
//!
//! // Get a output stream handle to the default physical sound device
//! let (_stream, stream_handle) = OutputStream::try_default().unwrap();
//! // Load a sound from a file, using a path relative to Cargo.toml
//! let file = BufReader::new(File::open("examples/music.ogg").unwrap());
//! // Decode that sound file into a source
//! let source = Decoder::new(file).unwrap();
//! // Play the sound directly on the device
//! stream_handle.play_raw(source.convert_samples());
//!
//! // The sound plays in a separate audio thread,
//! // so we need to keep the main thread alive while it's playing.
//! std::thread::sleep(std::time::Duration::from_secs(5));
//! ```
//!
//! ## Sink
//!
//! In order to make it easier to control the playback, the rodio library also provides a type
//! named [`Sink`] which represents an audio track.
//!
//! Instead of playing the sound with [`play_raw`](OutputStreamHandle::play_raw), you can add it to
//! a [`Sink`] instead.
//!
//! - Get a [`Sink`] to the output stream, and [`.append()`](Sink::append) your sound to it.
//!
//! ```no_run
//! use std::fs::File;
//! use std::io::BufReader;
//! use std::time::Duration;
//! use rodio::{Decoder, OutputStream, Sink};
//! use rodio::source::{SineWave, Source};
//!
//! let (_stream, stream_handle) = OutputStream::try_default().unwrap();
//! let sink = Sink::try_new(&stream_handle).unwrap();
//!
//! // Add a dummy source of the sake of the example.
//! let source = SineWave::new(440).take_duration(Duration::from_secs_f32(0.25)).amplify(0.20);
//! sink.append(source);
//!
//! // The sound plays in a separate thread. This call will block the current thread until the sink
//! // has finished playing all its queued sounds.
//! sink.sleep_until_end();
//! ```
//!
//! The [`append`](Sink::append) method will add the sound at the end of the
//! sink. It will be played when all the previous sounds have been played. If you want multiple
//! sounds to play simultaneously, you should create multiple [`Sink`]s.
//!
//! The [`Sink`] type also provides utilities such as playing/pausing or controlling the volume.
//!
//! ## Filters
//!
//! The [`Source`] trait provides various filters, similar to the standard [`Iterator`] trait.
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
//! the output to the device. Whenever you give up ownership of a [`Source`] in order to play it,
//! it is sent to this background thread where it will be read by rodio.
//!
//! All the sounds are mixed together by rodio before being sent to the operating system or the
//! hardware. Therefore there is no restriction on the number of sounds that play simultaneously or
//! the number of sinks that can be created (except for the fact that creating too many will slow
//! down your program).
//!
#![cfg_attr(test, deny(missing_docs))]
pub use cpal::{
    self, traits::DeviceTrait, Device, Devices, DevicesError, InputDevices, OutputDevices,
    SupportedStreamConfig,
};

mod conversions;
mod sink;
mod spatial_sink;
mod stream;

pub mod buffer;
pub mod decoder;
pub mod dynamic_mixer;
pub mod queue;
pub mod source;
pub mod static_buffer;

pub use crate::conversions::Sample;
pub use crate::decoder::Decoder;
pub use crate::sink::Sink;
pub use crate::source::Source;
pub use crate::spatial_sink::SpatialSink;
pub use crate::stream::{OutputStream, OutputStreamHandle, PlayError, StreamError};
