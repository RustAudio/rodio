# Version 0.17.0 (2022-09-14)

- Update `cpal` to [0.15](https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md#version-0150-2022-01-29).
- Default to `symphonia` for mp3 decoding.

# Version 0.16.0 (2022-09-14)

- Update `cpal` to [0.14](https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md#version-0140-2022-08-22).
- Update `symphonia` to [0.5](https://github.com/pdeljanov/Symphonia/releases/tag/v0.5.1).

# Version 0.15.0 (2022-01-23)

- Remove requirement that the argument `Decoder::new` and `LoopedDecoder::new` implement `Send`.
- Add optional symphonia backend.
- `WavDecoder`'s `total_duration` now returns the total duration of the sound rather than the remaining duration.
- Add 32-bit signed in WAV decoding.
- `SineWave::new()` now takes a `f32` instead of a `u32`.
- Add `len()` method to `SpatialSink`.

# Version 0.14.0 (2021-05-21)

- Re-export `cpal` in full.
- Replace panics when calling `OutputStream::try_default`, `OutputStream::try_from_device` with new
  `StreamError` variants.
- `OutputStream::try_default` will now fallback to non-default output devices if an `OutputStream`
  cannot be created from the default device.

# Version 0.13.1 (2021-03-28)

- Fix panic when no `pulseaudio-alsa` was installed.

# Version 0.13.0 (2020-11-03)

- Update `cpal` to [0.13](https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md#version-0130-2020-10-28).
- Add Android support.

# Version 0.12.0 (2020-10-05)

- Breaking: Update `cpal` to [0.12](https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md#version-0120-2020-07-09).
- Breaking: Rework API removing global "rodio audio processing" thread & adapting to the upstream cpal API changes.
- Add new_X format specific methods to Decoder.
- Fix resampler dependency on internal `Vec::capacity` behaviour.

# Version 0.11.0 (2020-03-16)

- Update `lewton` to [0.10](https://github.com/RustAudio/lewton/blob/master/CHANGELOG.md#release-0100---january-30-2020).
- Breaking: Update `cpal` to [0.11](https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md#version-0110-2019-12-11)

# Version 0.10.0 (2019-11-16)

- Removal of nalgebra in favour of own code.
- Fix a bug that switched channels when resuming after having paused.
- Attempt all supported output formats if the default format fails in `Sink::new`.
- Breaking: Update `cpal` to [0.10](https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md#version-0100-2019-07-05).

# Version 0.9.0 (2019-06-08)

- Remove exclusive `&mut` borrow requirements in `Sink` & `SpatialSink` setters.
- Use `nalgebra` instead of `cgmath` for `Spatial` source.

# Version 0.8.1 (2018-09-18)

- Update `lewton` dependency to [0.9](https://github.com/RustAudio/lewton/blob/master/CHANGELOG.md#release-090---august-16-2018)
- Change license from `Apache-2.0` only to `Apache-2.0 OR MIT`

# Version 0.8.0 (2018-06-22)

- Add mp3 decoding capabilities via `minimp3`

# Version 0.7.0 (2018-04-19)

- Update `cpal` dependency to 0.8, and adopt the new naming convention
- BREAKING CHANGES:
    - renamed `Endpoint` to `Device`
    - split `default_endpoint()` into `default_output_device()` and `default_input_device()`
    - renamed `endpoints()` to `devices()`
    - introduced `output_devices()` and `input_devices()`
