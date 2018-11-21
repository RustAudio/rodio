# Unreleased
- Remove exclusive `&mut` borrow requirements in `Sink` & `SpatialSink` setters
- Update `claxon` dependency to 0.4
- Update `cgmath` to 0.16

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
