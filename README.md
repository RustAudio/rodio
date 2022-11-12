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

## License
[License]: #license

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0), or
* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### License of your contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
