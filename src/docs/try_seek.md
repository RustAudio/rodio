Attempts to seek to a given position in the current source.

As long as the duration of the source is known, seek is guaranteed to saturate
at the end of the source. For example given a source that reports a total duration
of 42 seconds calling `try_seek()` with 60 seconds as argument will seek to
42 seconds.

# Errors
This function will return [`SeekError::NotSupported`] if one of the underlying
sources does not support seeking.

It will return an error if an implementation ran
into one during the seek.

Seeking beyond the end of a source might return an error if the total duration of
the source is not known.
