This guide will help you update your code when upgrading from older versions of rodio.

# rodio 0.20.1 or earlier to current GitHub version

## Features
- If you use disable the rodio features with `default_features = false` in
  `Cargo.toml` you need to add a new feature `playback`.

## Source implementations
- Source had a required method `current_frame_len`. In the latest version of rodio *frame* has been renamed to *span*. You will need to change every occurrence of `current_frame_len` to `current_span_len`.

## OutputStream
- The outputstream is now more configurable. Where you used `OutputStream::try_default()` you have a choice:
    - *(recommended)* Get an error when the default stream could not be opened: `OutputStreamBuilder::open_default_stream()?`
    - Stay close to the old behavior using:
      `OutputStreamBuilder::open_stream_or_fallback()`, which tries to open the
      default (audio) stream. If that fails it tries all other combinations of
      device and settings. The old behavior was only trying all settings of the
      default device.
