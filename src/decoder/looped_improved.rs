use std::{
    io::{Read, Seek},
    marker::PhantomData,
    sync::Arc,
    time::Duration,
};

use crate::{
    common::{ChannelCount, SampleRate},
    math::nz,
    source::{SeekError, Source},
    BitDepth, Sample,
};

use super::{builder::Settings, DecoderError, DecoderImpl};

#[cfg(feature = "claxon")]
use super::flac;
#[cfg(feature = "minimp3")]
use super::mp3;
#[cfg(feature = "symphonia")]
use super::symphonia;
#[cfg(feature = "lewton")]
use super::vorbis;
#[cfg(feature = "hound")]
use super::wav;

/// Decoder that loops indefinitely by seeking back to the start when reaching the end.
///
/// Uses fast seeking for seekable sources with gapless playback, otherwise recreates the
/// decoder while caching metadata to avoid expensive file scanning.
pub struct LoopedDecoder<R: Read + Seek> {
    pub(super) inner: Option<DecoderImpl<R>>,
    pub(super) settings: Settings,
    cached_duration: Option<Duration>,
}

impl<R> LoopedDecoder<R>
where
    R: Read + Seek,
{
    pub(super) fn new(decoder: DecoderImpl<R>, settings: Settings) -> Self {
        Self {
            inner: Some(decoder),
            settings,
            cached_duration: None,
        }
    }

    /// Recreates decoder with cached metadata to avoid expensive file scanning.
    fn recreate_decoder_with_cache(
        &mut self,
        decoder: DecoderImpl<R>,
    ) -> Option<(DecoderImpl<R>, Option<Sample>)> {
        let mut fast_settings = self.settings.clone();
        fast_settings.total_duration = self.cached_duration;

        let (new_decoder, sample) = match decoder {
            #[cfg(feature = "hound")]
            DecoderImpl::Wav(source) => {
                let mut reader = source.into_inner();
                reader.rewind().ok()?;
                let mut source = wav::WavDecoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                (DecoderImpl::Wav(source), sample)
            }
            #[cfg(feature = "lewton")]
            DecoderImpl::Vorbis(source) => {
                let mut reader = source.into_inner().into_inner().into_inner();
                reader.rewind().ok()?;
                let mut source =
                    vorbis::VorbisDecoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                (DecoderImpl::Vorbis(source), sample)
            }
            #[cfg(feature = "claxon")]
            DecoderImpl::Flac(source) => {
                let mut reader = source.into_inner();
                reader.rewind().ok()?;
                let mut source =
                    flac::FlacDecoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                (DecoderImpl::Flac(source), sample)
            }
            #[cfg(feature = "minimp3")]
            DecoderImpl::Mp3(source) => {
                let mut reader = source.into_inner();
                reader.rewind().ok()?;
                let mut source = mp3::Mp3Decoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                (DecoderImpl::Mp3(source), sample)
            }
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source, PhantomData) => {
                let mut reader = source.into_inner();
                reader.rewind().ok()?;
                let mut source =
                    symphonia::SymphoniaDecoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                (DecoderImpl::Symphonia(source, PhantomData), sample)
            }
            DecoderImpl::None(_, _) => return None,
        };
        Some((new_decoder, sample))
    }
}

impl<R> Iterator for LoopedDecoder<R>
where
    R: Read + Seek,
{
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(inner) = &mut self.inner {
            if let Some(sample) = inner.next() {
                return Some(sample);
            }

            // Cache duration on first loop to avoid recalculation
            if self.cached_duration.is_none() {
                self.cached_duration = inner.total_duration();
            }

            // Fast gapless seeking when available
            if self.settings.gapless
                && self.settings.is_seekable
                && inner.try_seek(Duration::ZERO).is_ok()
            {
                return inner.next();
            }

            // Recreation fallback with cached metadata
            let decoder = self.inner.take()?;
            let (new_decoder, sample) = self.recreate_decoder_with_cache(decoder)?;
            self.inner = Some(new_decoder);
            sample
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (
            self.inner.as_ref().map_or(0, |inner| inner.size_hint().0),
            None, // Infinite
        )
    }
}

impl<R> Source for LoopedDecoder<R>
where
    R: Read + Seek,
{
    fn current_span_len(&self) -> Option<usize> {
        self.inner.as_ref()?.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.as_ref().map_or(nz!(1), |inner| inner.channels())
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner
            .as_ref()
            .map_or(nz!(44100), |inner| inner.sample_rate())
    }

    /// Always returns `None` since looped decoders have no fixed end.
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    fn bits_per_sample(&self) -> Option<BitDepth> {
        self.inner.as_ref()?.bits_per_sample()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        match &mut self.inner {
            Some(inner) => inner.try_seek(pos),
            None => Err(SeekError::Other(Arc::new(DecoderError::IoError(
                "Looped source ended when it failed to loop back".to_string(),
            )))),
        }
    }
}