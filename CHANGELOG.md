# Unreleased
- Update `lewton` to 0.10.

# Version 0.10.0 (2019-11-16)
- Removal of nalgebra in favour of own code.
- Fix a bug that switched channels when resuming after having paused.
- Attempt all supported output formats if the default format fails in `Sink::new`.
- Breaking: Update `cpal` to [0.10](https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md#version-0100-2019-07-05).

# Version 0.9.0 (2019-06-08)

- Remove exclusive `&mut` borrow requirements in `Sink` & `SpatialSink` setters.
- Use `nalgebra` instead of `cgmath` for `Spatial` source.

# Version 0.8.1 (2018-09-18)

- Update `lewton` dependency to 0.9
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
