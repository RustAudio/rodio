use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

use crate::source::SeekError;
use crate::Source;

use crate::common::{ChannelCount, SampleRate};
use hound::{SampleFormat, WavReader};

/// Decoder for the WAV format.
pub struct WavDecoder<R>
where
    R: Read + Seek,
{
    reader: SamplesIterator<R>,
    total_duration: Duration,
    sample_rate: SampleRate,
    channels: ChannelCount,
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
        let len = reader.len() as u64;
        let reader = SamplesIterator {
            reader,
            samples_read: 0,
        };

        let sample_rate = spec.sample_rate;
        let channels = spec.channels;
        let total_duration =
            Duration::from_micros((1_000_000 * len) / (sample_rate as u64 * channels as u64));

        Ok(WavDecoder {
            reader,
            total_duration,
            sample_rate: sample_rate as SampleRate,
            channels: channels as ChannelCount,
        })
    }
    pub fn into_inner(self) -> R {
        self.reader.reader.into_inner()
    }
}

struct SamplesIterator<R>
where
    R: Read + Seek,
{
    reader: WavReader<R>,
    samples_read: u32, // wav header is u32 so this suffices
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
            (SampleFormat::Int, 8) => self.reader.samples().next().map(|value| {
                self.samples_read += 1;
                i8_to_i16(value.unwrap_or(0))
            }),
            (SampleFormat::Int, 16) => self.reader.samples().next().map(|value| {
                self.samples_read += 1;
                value.unwrap_or(0)
            }),
            (SampleFormat::Int, 24) => self.reader.samples().next().map(|value| {
                self.samples_read += 1;
                i24_to_i16(value.unwrap_or(0))
            }),
            (SampleFormat::Int, 32) => self.reader.samples().next().map(|value| {
                self.samples_read += 1;
                i32_to_i16(value.unwrap_or(0))
            }),
            (sample_format, bits_per_sample) => {
                panic!("Unimplemented wav spec: {sample_format:?}, {bits_per_sample}")
            }
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
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.channels
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        Some(self.total_duration)
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        let file_len = self.reader.reader.duration();

        let new_pos = pos.as_secs_f32() * self.sample_rate() as f32;
        let new_pos = new_pos as u32;
        let new_pos = new_pos.min(file_len); // saturate pos at the end of the source

        // make sure the next sample is for the right channel
        let to_skip = self.reader.samples_read % self.channels() as u32;

        self.reader
            .reader
            .seek(new_pos)
            .map_err(SeekError::HoundDecoder)?;
        self.reader.samples_read = new_pos * self.channels() as u32;

        for _ in 0..to_skip {
            self.next();
        }

        Ok(())
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
    let stream_pos = data.stream_position().unwrap();

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
    (f.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

/// Returns an 8-bit WAV int as an i16. This scales the sample value by a factor
/// of 256.
fn i8_to_i16(i: i8) -> i16 {
    i as i16 * 256
}

/// Returns a 24 bit WAV int as an i16. Note that this is a 24 bit integer, not a
/// 32 bit one. 24 bit ints are in the range [−8,388,608, 8,388,607] while i16s
/// are in the range [-32768, 32767]. Note that this function definitely causes
/// precision loss but hopefully this isn't too audiable when actually playing?
fn i24_to_i16(i: i32) -> i16 {
    (i >> 8) as i16
}

/// Returns a 32 bit WAV int as an i16. 32 bit ints are in the range
/// [-2,147,483,648, 2,147,483,647] while i16s are in the range [-32768, 32767].
/// Note that this function definitely causes precision loss but hopefully this
/// isn't too audiable when actually playing?
fn i32_to_i16(i: i32) -> i16 {
    (i >> 16) as i16
}
