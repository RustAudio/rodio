# Version 0.7.0 (2018-04-19)

- Update `cpal` dependency to 0.8, and adopt the new naming convention
- BREAKING CHANGES:
    - renamed `Endpoint` to `Device`
    - splitted `default_endpoint()` into `default_output_device()` and `default_input_device()`
    - renamed `endpoints()` to `devices()`
    - introduced `output_devices()` and `input_devices()`
