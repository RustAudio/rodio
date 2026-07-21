//! Audio playback library.
//!
//! The main concept of this library is the [`Source`] trait, which
//! represents a sound (streaming or not). In order to play a sound, there are three steps:
//!
//! - Get an OS-Sink handle to a physical device. For example, get a sink to the system's
//!   default sound device with [`DeviceSinkBuilder::open_default_sink()`].
//! - Create an object that represents the streaming sound. It can be a sine wave, a buffer, a
//!   [`decoder`], etc. or even your own type that implements the [`Source`] trait.
//! - Add the source to the OS-Sink using [`MixerDeviceSink::mixer()`]
//!   on the OS-Sink handle.
//!
//! Here is a complete example of how you would play an audio file:
//!
#![cfg_attr(not(feature = "playback"), doc = "```ignore")]
#![cfg_attr(feature = "playback", doc = "```no_run")]
//! use std::fs::File;
//! use rodio::{Decoder, MixerDeviceSink, source::Source};
//!
//! // Get an OS-Sink handle to the default physical sound device.
//! // Note that the playback stops when the handle is dropped.//!
//! let handle = rodio::DeviceSinkBuilder::open_default_sink()
//!         .expect("open default audio stream");
//! let player = rodio::Player::connect_new(&handle.mixer());
//! // Load a sound from a file, using a path relative to Cargo.toml
//! let file = File::open("examples/music.ogg").unwrap();
//! // Decode that sound file into a source
//! let source = Decoder::try_from(file).unwrap();
//! // Play the sound directly on the device
//! handle.mixer().add(source);
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
//! use rodio::{Decoder, MixerDeviceSink, source::Source};
//!
//! // Get an OS-Sink handle to the default physical sound device.
//! // Note that the playback stops when the sink_handle is dropped.
//! let sink_handle = rodio::DeviceSinkBuilder::open_default_sink()
//!         .expect("open default audio stream");
//!
//! // Load a sound from a file, using a path relative to Cargo.toml
//! let file = BufReader::new(File::open("examples/music.ogg").unwrap());
//! // Note that the playback stops when the player is dropped
//! let player = rodio::play(&sink_handle.mixer(), file).unwrap();
//!
//! // The sound plays in a separate audio thread,
//! // so we need to keep the main thread alive while it's playing.
//! std::thread::sleep(std::time::Duration::from_secs(5));
//! ```
//!
//!
//! ## Player
//!
//! In order to make it easier to control the playback, the rodio library also provides a type
//! named [`Player`] which represents an audio track. [`Player`] plays its input sources sequentially,
//! one after another. To play sounds in simultaneously in parallel, use [`mixer::Mixer`] instead.
//!
//! To play a song Create a [`Player`], connect it to the OS-Sink,
//! and [`.append()`](Player::append) your sound to it.
//!
#![cfg_attr(not(feature = "playback"), doc = "```ignore")]
#![cfg_attr(feature = "playback", doc = "```no_run")]
//! use std::time::Duration;
//! use rodio::{MixerDeviceSink, Player};
//! use rodio::source::{SineWave, Source};
//!
//! // _stream must live as long as the sink
//! let handle = rodio::DeviceSinkBuilder::open_default_sink()
//!         .expect("open default audio stream");
//! let player = rodio::Player::connect_new(&handle.mixer());
//!
//! // Add a dummy source of the sake of the example.
//! let source = SineWave::new(440.0).take_duration(Duration::from_secs_f32(0.25)).amplify(0.20);
//! player.append(source);
//!
//! // The sound plays in a separate thread. This call will block the current thread until the
//! // player has finished playing all its queued sounds.
//! player.sleep_until_end();
//! ```
//!
//! The [`append`](Player::append) method will add the sound at the end of the
//! player. It will be played when all the previous sounds have been played. If you want multiple
//! sounds to play simultaneously consider building your own [`Player`] from rodio parts.
//!
//! The [`Player`] type also provides utilities such as playing/pausing or controlling the volume.
//!
//! <div class="warning">Note that playback through Player will end if the associated
//! DeviceSink is dropped.</div>
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
//! ## Decoder Backends
//!
//! [Symphonia](https://github.com/pdeljanov/Symphonia) is the default decoder library.
//! Rodio supports enabling all of Symphonia's codecs using the `symphonia-all` feature
//! or enabling specific codecs using one of the `symphonia-{codec name}` features.
//! By default, decoders for the most common file types (flac, mp3, mp4, vorbis, wav) are enabled.
//!
//! See the [symphonia docs](https://docs.rs/symphonia/latest/symphonia) for the complete
//! list of supported formats.
//!
//! ### Alternative Decoders
//!
//! Alternative decoder libraries are available for some filetypes:
//!
//! - claxon (flac)
//! - hound (wav)
//! - lewton (vorbis)
//! - minimp3 (mp3)
//!
//! Symphonia is recommended for most usage. However, the alternative decoders are all licensed
//! under either MIT or Apache 2.0 while Symphonia is licensed under MPL-2.0, so
//! this may be a factor if you have strict licensing requirements.
//!
//! If you enable one of these, you may want to set `default-features = false`
//! to avoid adding extra crates to your binary.
//! See the [available feature flags](https://docs.rs/crate/rodio/latest/features) for all options.
//!
//! ## Feature Flags
//!
//! ### Core
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `playback` **(default)** | Audio playback |
//! | `recording` **(default)** | Microphone input |
//! | `wav_output` | Write decoded audio to a WAV file |
//! | `tracing` | Record stream errors as `tracing` events instead of printing to stderr |
//! | `experimental` | Experimental APIs backed by atomic floating-point operations |
//!
//! ### Performance
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `simd` **(default)** | SIMD-accelerated decoding |
//! | `64bit` | Use `f64` instead of `f32` for all samples and internal math [^1] |
//! | `realtime` | Real-time thread scheduling priority (you must grant `rtprio` yourself) |
//! | `realtime-dbus` | Like `realtime`, but uses D-Bus/rtkit to arrange limits automatically on desktop Linux |
//!
//! [^1]: By default, rodio uses 32-bit floats (`f32`), which offers better performance and is
//! sufficient for most use cases. The 64-bit mode addresses precision drift when chaining many
//! audio operations together, in long-running signal generators where phase errors compound
//! over time, and during resampling where rounding errors accumulate.
//!
//! ### Audio generation
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `noise` | White and pink noise sources |
//! | `dither` | Dithering; implies `noise` |
//!
//! ### Platform backends
//!
//! | Feature | Platform | Description |
//! |---------|----------|-------------|
//! | `pulseaudio` **(default)** | Linux | PulseAudio backend; also works on PipeWire systems via `pipewire-pulse`, with ALSA as fallback |
//! | `asio` | Windows | ASIO low-latency backend |
//! | `jack` | Linux/macOS/Windows | JACK Audio Connection Kit |
//! | `pipewire` | Linux | Native PipeWire backend |
//! | `audio-worklet` | Wasm | Audio Worklet backend |
//! | `wasm-bindgen` | Wasm | wasm-bindgen Wasm backend |
//!
//! ### Audio format features
//!
//! See the [Decoder Backends](#decoder-backends) section above.
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
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "playback")]
pub use cpal::{
    self, traits::DeviceTrait, Device, Devices, Error as CpalError, ErrorKind as CpalErrorKind,
    InputDevices, OutputDevices, SupportedStreamConfig,
};

mod common;
mod player;
mod spatial_player;
#[cfg(all(feature = "playback", feature = "experimental"))]
pub mod speakers;
#[cfg(feature = "playback")]
pub mod stream;
#[cfg(feature = "wav_output")]
mod wav_output;

pub mod buffer;
pub mod conversions;
pub mod decoder;

pub mod fixed_source;
pub mod const_source;

pub mod generators;

pub mod math;
#[cfg(feature = "recording")]
/// Microphone input support for audio recording.
pub mod microphone;
pub mod mixer;
pub mod queue;
pub mod source;
pub mod static_buffer;

pub use crate::common::{BitDepth, ChannelCount, Float, Sample, SampleRate, DEFAULT_SAMPLE_RATE};
pub use crate::decoder::Decoder;
pub use crate::fixed_source::FixedSource;
pub use crate::const_source::ConstSource;
pub use crate::player::Player;
pub use crate::source::Source;
pub use crate::spatial_player::SpatialPlayer;
#[cfg(feature = "playback")]
pub use crate::stream::{play, DeviceSinkBuilder, DeviceSinkError, MixerDeviceSink, PlayError};
#[cfg(feature = "wav_output")]
pub use crate::wav_output::wav_to_file;
#[cfg(feature = "wav_output")]
pub use crate::wav_output::wav_to_writer;

pub use Source as DynamicSource;
