This guide will help you update your code when upgrading from older versions of rodio.

# rodio 0.21 to current GitHub version

nothing

# rodio 0.20 or earlier to 0.21

## Features
- If you use disable the rodio features with `default_features = false` in `Cargo.toml` you need to
  add a new feature `playback`.
- The default decoders have changed to Symphonia, which itself is licensed under MPL. If you want
  to revert to the old decoders, you need to disable default-features and enable the `claxon`, `hound` and `lewton` features in `Cargo.toml` for respectively FLAC, WAV and Ogg Vorbis.

## Source implementations
- Source had a required method `current_frame_len`. In the latest version of rodio *frame* has been renamed to *span*. You will need to change every occurrence of `current_frame_len` to `current_span_len`.
- Source was generic over sample type. It no longer is. 
    - Remove any generics (`::<f32>`, `::<u16>` or `::<i16>`) that cause errors. 
    - Remove `SampleConvertor` it is no longer needed and has been removed.
    - Remove any calls to `source.convert_samples()` they are no longer needed and
      removed

## OutputStream
- The output stream is now more configurable. Where you used `OutputStream::try_default()` you have a choice:
    - *(recommended)* Get an error when the default stream could not be opened: `OutputStreamBuilder::open_default_stream()?`
    - Stay close to the old behavior using:
      `OutputStreamBuilder::open_stream_or_fallback()`, which tries to open the
      default (audio) stream. If that fails it tries all other combinations of
      device and settings. The old behavior was only trying all settings of the
      default device.
    - The output stream now prints to stderr or logs a message on drop, if that breaks your
      CLI/UI use `stream.log_on_drop(false)`.

## Sink & SpatialSink
- Replace `Sink::try_new` with `sink::connect_new`. It now takes an `&Mixer`
instead of a `OutputStreamHandle`. You get an `&Mixer` by calling `mixer()` on
`OutputStream`.
- Replace `Sink::new_idle` with `Sink::new`, 

### Example
The following Rodio *0.20.1* code:
```rust
let (_stream, handle) = rodio::OutputStream::try_default()?;
let sink = rodio::Sink::try_new(&handle)?;
```
Should be written like this in Rodio *0.21.0*:
```rust
let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
let sink = rodio::Sink::connect_new(stream_handle.mixer());
```

The `SpatialSink` changes mirror those in `Sink` described above.

## Dynamic mixer
Replace `DynamicMixerController` with `Mixer` and `DynamicMixer` with `MixerSource`.

## Decoder
- `Decoder::new_mp4` no longer takes an `Mp4Type` as hint. You can remove the argument
- Symphonia now longer assumes all sources are seek-able. Use
  `DecoderBuilder::with_seekable` or `try_from` on a `File` or `Bufreader`.

The following Rodio *0.20.1* code
```rust
let file = File::open("music.ogg")?;
let reader = BufReader::new(file);
let source = Decoder::new(reader);
```
Should be written like this in Rodio *0.21.0*:
```rust
let file = File::open("music.ogg")?;
let source = Decoder::try_from(music.ogg)?;
```
