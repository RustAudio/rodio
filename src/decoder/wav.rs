use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

use Source;

use hound::WavReader;

/// Decoder for the WAV format.
pub struct WavDecoder<R>
    where R: Read + Seek
{
    reader: SamplesIterator<R>,
    samples_rate: u32,
    channels: u16,
}

impl<R> WavDecoder<R>
    where R: Read + Seek
{
    /// Attempts to decode the data as WAV.
    pub fn new(mut data: R) -> Result<WavDecoder<R>, R> {
        if !is_wave(data.by_ref()) {
            return Err(data);
        }

        let reader = WavReader::new(data).unwrap();
        let spec = reader.spec();
        let reader = SamplesIterator {
            reader: reader,
            samples_read: 0,
        };

        Ok(WavDecoder {
            reader: reader,
            samples_rate: spec.sample_rate,
            channels: spec.channels,
        })
    }
}

struct SamplesIterator<R>
    where R: Read + Seek
{
    reader: WavReader<R>,
    samples_read: u32,
}

impl<R> Iterator for SamplesIterator<R>
    where R: Read + Seek
{
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        if let Some(value) = self.reader.samples().next() {
            self.samples_read += 1;
            Some(value.unwrap_or(0))
        } else {
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = (self.reader.len() - self.samples_read) as usize;
        (len, Some(len))
    }
}

impl<R> ExactSizeIterator for SamplesIterator<R> where R: Read + Seek {}

impl<R> Source for WavDecoder<R>
    where R: Read + Seek
{
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        self.channels
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        self.samples_rate
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        let ms = self.len() * 1000 / (self.channels as usize * self.samples_rate as usize);
        Some(Duration::from_millis(ms as u64))
    }
}

impl<R> Iterator for WavDecoder<R>
    where R: Read + Seek
{
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        self.reader.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.reader.size_hint()
    }
}

impl<R> ExactSizeIterator for WavDecoder<R> where R: Read + Seek {}

/// Returns true if the stream contains WAV data, then resets it to where it was.
fn is_wave<R>(mut data: R) -> bool
    where R: Read + Seek
{
    let stream_pos = data.seek(SeekFrom::Current(0)).unwrap();

    if WavReader::new(data.by_ref()).is_err() {
        data.seek(SeekFrom::Start(stream_pos)).unwrap();
        return false;
    }

    data.seek(SeekFrom::Start(stream_pos)).unwrap();
    true
}
