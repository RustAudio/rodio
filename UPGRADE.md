This guide will help you update your code when upgrading from older versions of rodio.

# rodio 0.21 to current GitHub version

nothing

# rodio 0.20 or earlier to 0.21

## Features
- If you disable the rodio default features with `default_features = false` in `Cargo.toml` you need to add a new feature `playback`.

## Source implementations
- Source had a required method `current_frame_len`. In the latest version of rodio *frame* has been renamed to *span*. You will need to change every occurrence of `current_frame_len` to `current_span_len`.
- Source was generic over sample type. It no longer is. 
    - Remove any generics (`::<f32>`, `::<u16>` or `::<i16>`) that cause errors. 
    - Remove `SampleConvertor` it is no longer needed and has been removed.
    - Remove any calls to `source.convert_samples()` they are no longer needed and
      removed

## OutputStream
The output stream is now more configurable. Where you used `OutputStream::try_default()` you have a choice:
 - *(recommended)* Get an error when the default stream could not be opened: `OutputStreamBuilder::open_default_stream()?`
 - Stay close to the old behavior using: `OutputStreamBuilder::open_stream_or_fallback()`, which tries to open the default (audio) stream. If that fails it tries all other combinations of device and settings. The old behavior was only trying all settings of the default device.

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
becomes this in Rodio *0.21.0*:
```rust
let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
let sink = rodio::Sink::connect_new(stream_handle.mixer());
```

The `SpatialSink` changes mirror those in `Sink` described above.

## Dynamic mixer
Replace `DynamicMixerController` with `Mixer` and `DynamicMixer` with `MixerSource`.

## Decoder
`Decoder::new_mp4` no longer takes an `Mp4Type` as hint. You can remove the argument

