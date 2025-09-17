use std::{
    io::{Read, Seek},
    sync::Arc,
    time::Duration,
};

#[cfg(feature = "symphonia")]
use std::marker::PhantomData;

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

/// Source of audio samples from decoding a file that never ends.
/// When the end of the file is reached, the decoder starts again from the beginning.
///
/// A `LoopedDecoder` will attempt to seek back to the start of the stream when it reaches
/// the end. If seeking fails for any reason (like IO errors), iteration will stop.
///
/// For seekable sources with gapless playback enabled, this uses `try_seek(Duration::ZERO)`
/// which is fast. For non-seekable sources or when gapless is disabled, it recreates the
/// decoder but caches metadata from the first iteration to avoid expensive file scanning
/// on subsequent loops.
///
/// # Examples
///
/// ```no_run
/// use std::fs::File;
/// use rodio::Decoder;
///
/// let file = File::open("audio.mp3").unwrap();
/// let looped_decoder = Decoder::new_looped(file).unwrap();
/// ```
#[allow(dead_code)]
pub struct LoopedDecoder<R: Read + Seek> {
    /// The underlying decoder implementation.
    pub(super) inner: Option<DecoderImpl<R>>,
    /// Configuration settings for the decoder.
    pub(super) settings: Settings,
    /// Used to avoid expensive file scanning on subsequent loops.
    cached_duration: Option<Duration>,
}

impl<R> LoopedDecoder<R>
where
    R: Read + Seek,
{
    /// Create a new `LoopedDecoder` with the given decoder and settings.
    pub(super) fn new(decoder: DecoderImpl<R>, settings: Settings) -> Self {
        Self {
            inner: Some(decoder),
            settings,
            cached_duration: None,
        }
    }

    /// Recreate decoder with cached metadata to avoid expensive file scanning.
    fn recreate_decoder_with_cache(
        &mut self,
        decoder: DecoderImpl<R>,
    ) -> Option<(DecoderImpl<R>, Option<Sample>)> {
        // Create settings with cached metadata for fast recreation.
        // Note: total_duration is important even though LoopedDecoder::total_duration()  returns
        // None, because the individual decoder's total_duration() is used for seek saturation
        // (clamping seeks beyond the end to the end position).
        let mut fast_settings = self.settings.clone();
        fast_settings.total_duration = self.cached_duration;

        match decoder {
            #[cfg(feature = "hound")]
            DecoderImpl::Wav(source) => {
                let mut reader = source.into_inner();
                reader.rewind().ok()?;
                let mut source = wav::WavDecoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                Some((DecoderImpl::Wav(source), sample))
            }
            #[cfg(feature = "lewton")]
            DecoderImpl::Vorbis(source) => {
                let mut reader = source.into_inner().into_inner().into_inner();
                reader.rewind().ok()?;
                let mut source =
                    vorbis::VorbisDecoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                Some((DecoderImpl::Vorbis(source), sample))
            }
            #[cfg(feature = "claxon")]
            DecoderImpl::Flac(source) => {
                let mut reader = source.into_inner();
                reader.rewind().ok()?;
                let mut source =
                    flac::FlacDecoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                Some((DecoderImpl::Flac(source), sample))
            }
            #[cfg(feature = "minimp3")]
            DecoderImpl::Mp3(source) => {
                let mut reader = source.into_inner();
                reader.rewind().ok()?;
                let mut source = mp3::Mp3Decoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                Some((DecoderImpl::Mp3(source), sample))
            }
            #[cfg(feature = "symphonia")]
            DecoderImpl::Symphonia(source, PhantomData) => {
                let mut reader = source.into_inner();
                reader.rewind().ok()?;
                let mut source =
                    symphonia::SymphoniaDecoder::new_with_settings(reader, &fast_settings).ok()?;
                let sample = source.next();
                Some((DecoderImpl::Symphonia(source, PhantomData), sample))
            }
            DecoderImpl::None(_, _) => None,
        }
    }
}

impl<R> Iterator for LoopedDecoder<R>
where
    R: Read + Seek,
{
    type Item = Sample;

    /// Returns the next sample in the audio stream.
    ///
    /// When the end of the stream is reached, attempts to seek back to the start and continue
    /// playing. For seekable sources with gapless playback, this uses fast seeking. For
    /// non-seekable sources or when gapless is disabled, recreates the decoder using cached
    /// metadata to avoid expensive file scanning.
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(inner) = &mut self.inner {
            if let Some(sample) = inner.next() {
                return Some(sample);
            }

            // Cache duration from current decoder before resetting (first time only)
            if self.cached_duration.is_none() {
                self.cached_duration = inner.total_duration();
            }

            // Try seeking first for seekable sources - this is fast and gapless
            // Only use fast seeking when gapless=true, otherwise recreate normally
            if self.settings.gapless
                && self.settings.is_seekable
                && inner.try_seek(Duration::ZERO).is_ok()
            {
                return inner.next();
            }

            // Fall back to recreation with cached metadata to avoid expensive scanning
            let decoder = self.inner.take()?;
            let (new_decoder, sample) = self.recreate_decoder_with_cache(decoder)?;
            self.inner = Some(new_decoder);
            sample
        } else {
            None
        }
    }

    /// Returns the size hint for this iterator.
    ///
    /// The lower bound is:
    /// - The minimum number of samples remaining in the current iteration if there is an active
    ///   decoder
    /// - 0 if there is no active decoder (inner is None)
    ///
    /// The upper bound is always `None` since the decoder loops indefinitely.
    ///
    /// Note that even with an active decoder, reaching the end of the stream may result in the
    /// decoder becoming inactive if seeking back to the start fails.
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (
            self.inner.as_ref().map_or(0, |inner| inner.size_hint().0),
            None,
        )
    }
}

impl<R> Source for LoopedDecoder<R>
where
    R: Read + Seek,
{
    /// Returns the current span length of the underlying decoder.
    ///
    /// Returns `None` if there is no active decoder.
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.inner.as_ref()?.current_span_len()
    }

    /// Returns the number of channels in the audio stream.
    ///
    /// Returns the default channel count if there is no active decoder.
    #[inline]
    fn channels(&self) -> ChannelCount {
        self.inner.as_ref().map_or(nz!(1), |inner| inner.channels())
    }

    /// Returns the sample rate of the audio stream.
    ///
    /// Returns the default sample rate if there is no active decoder.
    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.inner
            .as_ref()
            .map_or(nz!(44100), |inner| inner.sample_rate())
    }

    /// Returns the total duration of this audio source.
    ///
    /// Always returns `None` for looped decoders since they have no fixed end point -
    /// they will continue playing indefinitely by seeking back to the start when reaching
    /// the end of the audio data.
    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    /// Returns the bits per sample of the underlying decoder, if available.
    #[inline]
    fn bits_per_sample(&self) -> Option<BitDepth> {
        self.inner.as_ref()?.bits_per_sample()
    }

    /// Attempts to seek to a specific position in the audio stream.
    ///
    /// # Errors
    ///
    /// Returns `SeekError::NotSupported` if:
    /// - There is no active decoder
    /// - The underlying decoder does not support seeking
    ///
    /// May also return other `SeekError` variants if the underlying decoder's seek operation fails.
    ///
    /// # Note
    ///
    /// Even for looped playback, seeking past the end of the stream will not automatically
    /// wrap around to the beginning - it will return an error just like a normal decoder.
    /// Looping only occurs when reaching the end through normal playback.
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        match &mut self.inner {
            Some(inner) => inner.try_seek(pos),
            None => Err(SeekError::Other(Arc::new(DecoderError::IoError(
                "Looped source ended when it failed to loop back".to_string(),
            )))),
        }
    }
}
