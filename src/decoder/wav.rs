use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

use crate::Source;

use hound::{SampleFormat, WavReader};

/// Decoder for the WAV format.
pub struct WavDecoder<R>
where
    R: Read + Seek,
{
    reader: SamplesIterator<R>,
    sample_rate: u32,
    channels: u16,
}

impl<R> WavDecoder<R>
where
    R: Read + Seek,
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
            sample_rate: spec.sample_rate,
            channels: spec.channels,
        })
    }
}

struct SamplesIterator<R>
where
    R: Read + Seek,
{
    reader: WavReader<R>,
    samples_read: u32,
}

impl<R> Iterator for SamplesIterator<R>
where
    R: Read + Seek,
{
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        let spec = self.reader.spec();
        match (spec.sample_format, spec.bits_per_sample) {
            (SampleFormat::Float, 32) => self.reader.samples().next().map(|value| {
                self.samples_read += 1;
                f32_to_i16(value.unwrap_or(0.0))
            }),
            (SampleFormat::Int, 16) => self.reader.samples().next().map(|value| {
                self.samples_read += 1;
                value.unwrap_or(0)
            }),
            (SampleFormat::Int, 24) => self.reader.samples().next().map(|value| {
                self.samples_read += 1;
                i24_to_i16(value.unwrap_or(0))
            }),
            (sample_format, bits_per_sample) => panic!(
                "Unimplemented wav spec: {:?}, {}",
                sample_format, bits_per_sample
            ),
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
where
    R: Read + Seek,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.channels
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        let ms = self.len() * 1000 / (self.channels as usize * self.sample_rate as usize);
        Some(Duration::from_millis(ms as u64))
    }
}

impl<R> Iterator for WavDecoder<R>
where
    R: Read + Seek,
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
where
    R: Read + Seek,
{
    let stream_pos = data.seek(SeekFrom::Current(0)).unwrap();

    if WavReader::new(data.by_ref()).is_err() {
        data.seek(SeekFrom::Start(stream_pos)).unwrap();
        return false;
    }

    data.seek(SeekFrom::Start(stream_pos)).unwrap();
    true
}

/// Returns a 32 bit WAV float as an i16. WAV floats are typically in the range of
/// [-1.0, 1.0] while i16s are in the range [-32768, 32767]. Note that this
/// function definitely causes precision loss but hopefully this isn't too
/// audiable when actually playing?
fn f32_to_i16(f: f32) -> i16 {
    // prefer to clip the input rather than be excessively loud.
    (f.max(-1.0).min(1.0) * i16::max_value() as f32) as i16
}

/// Returns a 24 bit WAV int as an i16. Note that this is a 24 bit integer, not a
/// 32 bit one. 24 bit ints are in the range [−8,388,608, 8,388,607] while i16s
/// are in the range [-32768, 32767]. Note that this function definitely causes
/// precision loss but hopefully this isn't too audiable when actually playing?
fn i24_to_i16(i: i32) -> i16 {
    (i >> 8) as i16
}
