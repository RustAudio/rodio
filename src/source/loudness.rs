//! Perceptual loudness measurement (EBU R128 / ITU-R BS.1770).
//!
//! This source will pass audio through unchanged while measuring its loudness with the ebur128 crate.
//! This crate is a port of `libebur128` which produces results identical to the C reference (including K-weighting and gating).
//!
//! Read the current loudness in LUFS (Loudness Units relative to Full Scale) at any point via Loudness::momentary_lufs (in 400 ms window),
//! Loudness::short_term_lufs (in 3 s window) or Loudness::integrated_lufs (gated, whole-program).
//!
//! # Limitation right now:
//! The analyzer is configured for the stream's channel count and sample rate at the construction time.
//! Sources whose parameters change mid-stream (e.g. a queue of files with differing sample rates)
//! are not yet reconfigured; that would need the analyzer to be rebuilt at span boundaries.

use std::time::Duration;

use ebur128::{EbuR128, Mode};

use super::SeekError;
use crate::common::{ChannelCount, Float, SampleRate};
use crate::Source;

// ebu r128 measurement modes: momentary, short term and integrated loudness.
fn measurement_mode() -> Mode {
    Mode::M | Mode::S | Mode::I
}

/// passthrough source that measures perceptual loudness or LUFS.
///
/// The audio is forwarded unchanged. Read the current loudness via momentary_lufs, short_term_lufs or integrated_lufs
pub struct Loudness<I> {
    input: I,
    analyzer: EbuR128,
    channels: usize,
    // Accumulates one interleaved frame before handing it to the analyzer since ebur128 consumes whole frames!
    frame: Vec<Float>,
}

// construct a loudness passthrough source.
pub(crate) fn loudness<I>(input: I) -> Loudness<I>
where
    I: Source,
{
    let channels = input.channels();
    let sample_rate = input.sample_rate();

    let analyzer = EbuR128::new(channels.get() as u32, sample_rate.get(), measurement_mode())
        .expect("EbuR128 accepts any non-zero channel count and sample rate");

    Loudness {
        input,
        analyzer,
        channels: channels.get() as usize,
        frame: Vec::with_capacity(channels.get() as usize),
    }
}

impl<I> Loudness<I>
where
    I: Source,
{
    /// momentary loudness (in 400 ms window) in LUFS.
    /// will return f64::NEG_INFINITY until enough audio has been measured.
    #[inline]
    pub fn momentary_lufs(&self) -> f64 {
        self.analyzer
            .loudness_momentary()
            .unwrap_or(f64::NEG_INFINITY)
    }

    /// short term loudness (in 3 s window) in LUFS.
    #[inline]
    pub fn short_term_lufs(&self) -> f64 {
        self.analyzer
            .loudness_shortterm()
            .unwrap_or(f64::NEG_INFINITY)
    }

    /// integrated loudness in LUFS: gated, whole program.
    #[inline]
    pub fn integrated_lufs(&self) -> f64 {
        self.analyzer.loudness_global().unwrap_or(f64::NEG_INFINITY)
    }

    /// this returns a reference to inner source.
    #[inline]
    pub fn inner(&self) -> &I {
        &self.input
    }

    /// returns a mutable reference to inner source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.input
    }

    /// to unwrap this adapter, returning the inner source.
    #[inline]
    pub fn into_inner(self) -> I {
        self.input
    }

    // function to feed the buffered interleaved frame into the analyzer once complete.
    #[inline]
    fn feed(&mut self, temp: Float) {
        self.frame.push(temp);

        if self.frame.len() == self.channels {
            #[cfg(not(feature = "64bit"))]
            let result = self.analyzer.add_frames_f32(&self.frame);

            #[cfg(feature = "64bit")]
            let result = self.analyzer.add_frames_f64(&self.frame);

            // error will occur here only on allocation failure.
            // if we drop the frame the measurement just degrades slightly
            // otherwise it could break the playback.
            let _ = result;
            self.frame.clear();
        }
    }
}

impl<I> Iterator for Loudness<I>
where
    I: Source,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.input.next()?;
        self.feed(sample);
        Some(sample)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for Loudness<I> where I: Source + ExactSizeIterator {}

impl<I> Source for Loudness<I>
where
    I: Source,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.input.current_span_len()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.input.channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.input.try_seek(pos)?;
        // since loudness history is position-dependent, we restart the analyzer.
        if let Ok(fresh) = EbuR128::new(
            self.channels as u32,
            self.input.sample_rate().get(),
            measurement_mode(),
        ) {
            self.analyzer = fresh;
        }
        self.frame.clear();
        Ok(())
    }
}

impl<I: std::fmt::Debug> std::fmt::Debug for Loudness<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Loudness")
            .field("input", &self.input)
            .field("channels", &self.channels)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SineWave;

    #[test]
    fn passes_samples_through_unchanged() {
        let original: Vec<Float> = SineWave::new(440.0).take(500).collect();
        let measured: Vec<Float> = loudness(SineWave::new(440.0)).take(500).collect();
        assert_eq!(original, measured);
    }
}
