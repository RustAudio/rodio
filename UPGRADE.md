This guide will help you update your code when upgrading from rodio 0.20 or earlier to the current version.

## Source implementations
- Source had a required method `current_frame_len`. In the latest version of rodio *frame* has been renamed to *span*. You will need to change every occurrence of `current_frame_len` to `current_span_len`.

## OutputStream
- The outputstream is now more configurable. Where you used `OutputStream::try_default()` you have a choice:
    - *(recommended)* Get an error when the default stream could not be opened: `OutputStreamBuilder::open_default_stream()?`
    - Keep the old behavior using: `OutputStreamBuilder::open_stream_or_fallback()`, which tries to open the default (audio) stream trying others when the default stream could not be opened.
