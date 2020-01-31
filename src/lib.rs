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
//! let device = rodio::RodioDevice::default_output().unwrap();
//!
//! let file = File::open("sound.ogg").unwrap();
//! let source = rodio::Decoder::new(BufReader::new(file)).unwrap();
//! device.play_raw(source.convert_samples());
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
//! let device = rodio::RodioDevice::default_output().unwrap();
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
pub use cpal::{
    traits::DeviceTrait, Device, Devices, DevicesError, Format, InputDevices, OutputDevices,
};

pub use crate::conversions::Sample;
pub use crate::decoder::Decoder;
pub use crate::device::RodioDevice;
pub use crate::sink::Sink;
pub use crate::source::Source;
pub use crate::spatial_sink::SpatialSink;

mod conversions;
mod device;
mod device_mixer;
mod sink;
mod spatial_sink;

pub mod buffer;
pub mod decoder;
pub mod dynamic_mixer;
pub mod queue;
pub mod source;
pub mod static_buffer;
