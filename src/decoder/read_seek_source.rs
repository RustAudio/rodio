use std::io::{Read, Result, Seek, SeekFrom};

use symphonia::core::io::MediaSource;

pub struct ReadSeekSource<T: Read + Seek + Send + Sync> {
    inner: T,
    byte_len: Option<u64>,
}

impl<T: Read + Seek + Send + Sync> ReadSeekSource<T> {
    /// Instantiates a new `ReadSeekSource<T>` by taking ownership and wrapping the provided
    /// `Read + Seek`er.
    #[inline]
    pub fn new(inner: T, byte_len: Option<u64>) -> Self {
        ReadSeekSource { inner, byte_len }
    }
}

impl<T: Read + Seek + Send + Sync> MediaSource for ReadSeekSource<T> {
    #[inline]
    fn is_seekable(&self) -> bool {
        true
    }

    #[inline]
    fn byte_len(&self) -> Option<u64> {
        self.byte_len
    }
}

impl<T: Read + Seek + Send + Sync> Read for ReadSeekSource<T> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.inner.read(buf)
    }
}

impl<T: Read + Seek + Send + Sync> Seek for ReadSeekSource<T> {
    #[inline]
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        self.inner.seek(pos)
    }
}
