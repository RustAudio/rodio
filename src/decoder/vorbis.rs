use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

use crate::source::SeekError;
use crate::Source;

use lewton::inside_ogg::OggStreamReader;

/// Decoder for an OGG file that contains Vorbis sound format.
pub struct VorbisDecoder<R>
where
    R: Read + Seek,
{
    stream_reader: OggStreamReader<R>,
    current_data: Vec<i16>,
    next: usize,
}

impl<R> VorbisDecoder<R>
where
    R: Read + Seek,
{
    /// Attempts to decode the data as ogg/vorbis.
    pub fn new(mut data: R) -> Result<VorbisDecoder<R>, R> {
        if !is_vorbis(data.by_ref()) {
            return Err(data);
        }

        let stream_reader = OggStreamReader::new(data).unwrap();
        Ok(Self::from_stream_reader(stream_reader))
    }
    pub fn from_stream_reader(mut stream_reader: OggStreamReader<R>) -> Self {
        let mut data = match stream_reader.read_dec_packet_itl() {
            Ok(Some(d)) => d,
            _ => Vec::new(),
        };

        // The first packet is always empty, therefore
        // we need to read the second frame to get some data
        if let Ok(Some(mut d)) = stream_reader.read_dec_packet_itl() {
            data.append(&mut d);
        }

        VorbisDecoder {
            stream_reader,
            current_data: data,
            next: 0,
        }
    }
    pub fn into_inner(self) -> OggStreamReader<R> {
        self.stream_reader
    }
}

impl<R> Source for VorbisDecoder<R>
where
    R: Read + Seek,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.current_data.len())
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.stream_reader.ident_hdr.audio_channels as u16
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.stream_reader.ident_hdr.audio_sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    /// seek is broken, https://github.com/RustAudio/lewton/issues/73.
    // We could work around it by:
    //  - using unsafe to create an instance of Self
    //  - use mem::swap to turn the &mut self into a mut self
    //  - take out the underlying Read+Seek
    //  - make a new self and seek
    //
    // If this issue is fixed use the implementation in
    // commit: 3bafe32388b4eb7a48c6701e6c65044dc8c555e6
    #[inline]
    fn try_seek(&mut self, _: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported {
            underlying_source: std::any::type_name::<Self>(),
        })
    }
}

impl<R> Iterator for VorbisDecoder<R>
where
    R: Read + Seek,
{
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        if let Some(sample) = self.current_data.get(self.next).copied() {
            self.next += 1;
            if self.current_data.is_empty() {
                if let Ok(Some(data)) = self.stream_reader.read_dec_packet_itl() {
                    self.current_data = data;
                    self.next = 0;
                }
            }
            Some(sample)
        } else {
            if let Ok(Some(data)) = self.stream_reader.read_dec_packet_itl() {
                self.current_data = data;
                self.next = 0;
            }
            let sample = self.current_data.get(self.next).copied();
            self.next += 1;
            sample
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.current_data.len(), None)
    }
}

/// Returns true if the stream contains Vorbis data, then resets it to where it was.
fn is_vorbis<R>(mut data: R) -> bool
where
    R: Read + Seek,
{
    let stream_pos = data.stream_position().unwrap();

    if OggStreamReader::new(data.by_ref()).is_err() {
        data.seek(SeekFrom::Start(stream_pos)).unwrap();
        return false;
    }

    data.seek(SeekFrom::Start(stream_pos)).unwrap();
    true
}
