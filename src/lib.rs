//! Audio playback library.
//!
//! The main concept of this library is the [`Source`] trait, which
//! represents a sound (streaming or not). In order to play a sound, there are three steps:
//!
//! - Get an output stream handle to a physical device. For example, get a stream to the system's
//!   default sound device with [`OutputStreamBuilder::open_default_stream()`].
//! - Create an object that represents the streaming sound. It can be a sine wave, a buffer, a
//!   [`decoder`], etc. or even your own type that implements the [`Source`] trait.
//! - Add the source to the output stream using [`OutputStream::mixer()`](OutputStream::mixer)
//!   on the output stream handle.
//!
//! Here is a complete example of how you would play an audio file:
//!
#![cfg_attr(not(feature = "playback"), doc = "```ignore")]
#![cfg_attr(feature = "playback", doc = "```no_run")]
//! use std::fs::File;
//! use rodio::{Decoder, OutputStream, source::Source};
//!
//! // Get an output stream handle to the default physical sound device.
//! // Note that the playback stops when the stream_handle is dropped.//!
//! let stream_handle = rodio::OutputStreamBuilder::open_default_stream()
//!         .expect("open default audio stream");
//! let sink = rodio::Sink::connect_new(&stream_handle.mixer());
//! // Load a sound from a file, using a path relative to Cargo.toml
//! let file = File::open("examples/music.ogg").unwrap();
//! // Decode that sound file into a source
//! let source = Decoder::try_from(file).unwrap();
//! // Play the sound directly on the device
//! stream_handle.mixer().add(source);
//!
//! // The sound plays in a separate audio thread,
//! // so we need to keep the main thread alive while it's playing.
//! std::thread::sleep(std::time::Duration::from_secs(5));
//! ```
//!
//! [`rodio::play()`](crate::play) helps to simplify the above
#![cfg_attr(not(feature = "playback"), doc = "```ignore")]
#![cfg_attr(feature = "playback", doc = "```no_run")]
//! use std::fs::File;
//! use std::io::BufReader;
//! use rodio::{Decoder, OutputStream, source::Source};
//!
//! // Get an output stream handle to the default physical sound device.
//! // Note that the playback stops when the stream_handle is dropped.
//! let stream_handle = rodio::OutputStreamBuilder::open_default_stream()
//!         .expect("open default audio stream");
//!
//! // Load a sound from a file, using a path relative to Cargo.toml
//! let file = BufReader::new(File::open("examples/music.ogg").unwrap());
//! rodio::play(&stream_handle.mixer(), file).unwrap();
//!
//! // The sound plays in a separate audio thread,
//! // so we need to keep the main thread alive while it's playing.
//! std::thread::sleep(std::time::Duration::from_secs(5));
//! ```
//!
//!
//! ## Sink
//!
//! In order to make it easier to control the playback, the rodio library also provides a type
//! named [`Sink`] which represents an audio track. [`Sink`] plays its input sources sequentially,
//! one after another. To play sounds in simultaneously in parallel, use [`mixer::Mixer`] instead.
//!
//! To play a song Create a [`Sink`] connect it to the output stream,
//! and [`.append()`](Sink::append) your sound to it.
//!
#![cfg_attr(not(feature = "playback"), doc = "```ignore")]
#![cfg_attr(feature = "playback", doc = "```no_run")]
//! use std::time::Duration;
//! use rodio::{OutputStream, Sink};
//! use rodio::source::{SineWave, Source};
//!
//! // _stream must live as long as the sink
//! let stream_handle = rodio::OutputStreamBuilder::open_default_stream()
//!         .expect("open default audio stream");
//! let sink = rodio::Sink::connect_new(&stream_handle.mixer());
//!
//! // Add a dummy source of the sake of the example.
//! let source = SineWave::new(440.0).take_duration(Duration::from_secs_f32(0.25)).amplify(0.20);
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
//! **Please note that the [`Sink`] requires the [`OutputStream`], make sure that the OutputStream is not dropped before the sink.**
//!
//! ## Filters
//!
//! The [`Source`] trait provides various filters, similar to the standard [`Iterator`] trait.
//!
//! Example:
//!
#![cfg_attr(not(feature = "playback"), doc = "```ignore")]
#![cfg_attr(feature = "playback", doc = "```")]
//! use rodio::Source;
//! use std::time::Duration;
//!
//! // Repeats the first five seconds of the sound forever.
//! # let source = rodio::source::SineWave::new(440.0);
//! let source = source.take_duration(Duration::from_secs(5)).repeat_infinite();
//! ```
//!
//! ## Alternative Decoder Backends
//!
//! [Symphonia](https://github.com/pdeljanov/Symphonia) is an alternative decoder library that can be used in place
//! of many of the default backends.
//! Currently, the main benefit is that Symphonia is the only backend that supports M4A and AAC,
//! but it may be used to implement additional optional functionality in the future.
//!
//! To use, enable either the `symphonia-all` feature to enable all Symphonia codecs
//! or enable specific codecs using one of the `symphonia-{codec name}` features.
//! If you enable one or more of the Symphonia codecs, you may want to set `default-features = false` in order
//! to avoid adding extra crates to your binary.
//! See the [available feature flags](https://docs.rs/crate/rodio/latest/features) for all options.
//!
//! ## Optional Features
//!
//! Rodio provides several optional features that are guarded with feature gates.
//!
//! ### Feature "tracing"
//!
//! The "tracing" feature replaces the print to stderr when a stream error happens with a
//! recording an error event with tracing.
//!
//! ### Feature "noise"
//!
//! The "noise" feature adds support for white and pink noise sources. This feature requires the
//! "rand" crate.
//!
//! ### Feature "playback"
//!
//! The "playback" feature adds support for playing audio. This feature requires the "cpal" crate.
//!
//! ## How it works under the hood
//!
//! Rodio spawns a background thread that is dedicated to reading from the sources and sending
//! the output to the device. Whenever you give up ownership of a [`Source`] in order to play it,
//! it is sent to this background thread where it will be read by rodio.
//!
//! All the sounds are mixed together by rodio before being sent to the operating system or the
//! hardware. Therefore, there is no restriction on the number of sounds that play simultaneously or
//! on the number of sinks that can be created (except for the fact that creating too many will slow
//! down your program).

#![cfg_attr(
    any(test, not(feature = "playback")),
    deny(missing_docs),
    allow(dead_code),
    allow(unused_imports),
    allow(unused_variables),
    allow(unreachable_code)
)]

#[cfg(feature = "playback")]
pub use cpal::{
    self, traits::DeviceTrait, Device, Devices, DevicesError, InputDevices, OutputDevices,
    SupportedStreamConfig,
};

mod common;
mod sink;
mod spatial_sink;
#[cfg(feature = "playback")]
pub mod stream;
#[cfg(feature = "wav_output")]
mod wav_output;

pub mod buffer;
pub mod conversions;
pub mod decoder;
pub mod math;
pub mod mixer;
pub mod queue;
pub mod source;
pub mod static_buffer;

pub use crate::common::{ChannelCount, Sample, SampleRate};
pub use crate::decoder::Decoder;
pub use crate::sink::Sink;
pub use crate::source::Source;
pub use crate::spatial_sink::SpatialSink;
#[cfg(feature = "playback")]
pub use crate::stream::{play, OutputStream, OutputStreamBuilder, PlayError, StreamError};
#[cfg(feature = "wav_output")]
pub use crate::wav_output::output_to_wav;
