# Audio playback library

[![Crates.io Version](https://img.shields.io/crates/v/rodio.svg)](https://crates.io/crates/rodio)
[![Crates.io Downloads](https://img.shields.io/crates/d/rodio.svg)](https://crates.io/crates/rodio)
[![Build Status](https://github.com/RustAudio/rodio/workflows/CI/badge.svg)](https://github.com/RustAudio/rodio/actions)

Rust playback library.

Playback is handled by [cpal](https://github.com/RustAudio/cpal). Format decoding can be handled either by [Symphonia](https://github.com/pdeljanov/Symphonia), or by format-specific decoders:

 - MP3 by [minimp3](https://github.com/lieff/minimp3) (but defaults to [Symphonia](https://github.com/pdeljanov/Symphonia)).
 - WAV by [hound](https://github.com/ruud-v-a/hound).
 - Vorbis by [lewton](https://github.com/est31/lewton).
 - FLAC by [claxon](https://github.com/ruuda/claxon).
 - MP4 and AAC (both disabled by default) are handled only by [Symphonia](https://github.com/pdeljanov/Symphonia).

See [the docs](https://docs.rs/rodio/latest/rodio/#alternative-decoder-backends) for more details on backends.

# [Documentation](http://docs.rs/rodio)

[The documentation](http://docs.rs/rodio) contains an introduction to the library.

## Dependencies (Linux only)

Rodio uses `cpal` library to send audio to the OS for playback. ALSA development files are needed to build `cpal` on Linux. These are provided as part of the `libasound2-dev` package on Debian and Ubuntu distributions and `alsa-lib-devel` on Fedora.

### Minimal build

It is possible to build `rodio` without support for audio playback. In this configuration `cpal` dependency and its requirements are excluded. This configuration may be useful, for example, for decoding and processing audio in environments when the audio output is not available (e.g. in case of Linux, when ALSA is not available). See `into_file` example that works with this build.

In order to use `rodio` in this configuration disable default features and add the necessary ones. In this case the `Cargo.toml` dependency would look like:
```toml
[dependencies]
rodio = { version = "0.20.1", default-features = false, features = ["symphonia-all"] }
```


# Contributing

For information on how to contribute to this project, please see our [Contributing Guide](https://github.com/RustAudio/rodio/CONTRIBUTING.md).

## License
[License]: #license

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0), or
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### License of your contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
