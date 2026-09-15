//! Mixer that plays multiple sounds at the same time.

use crate::common::{ChannelCount, SampleRate};
use crate::source::{Empty, SeekError, Source, UniformSourceIterator};
use crate::Sample;
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "crossbeam-channel")]
use crossbeam_channel::{unbounded as channel, Receiver, Sender};
#[cfg(not(feature = "crossbeam-channel"))]
use std::sync::mpsc::{channel, Receiver, Sender};

#[cfg(test)]
mod tests;

/// Builds a new mixer.
///
/// You can choose the characteristics of the output thanks to this constructor. All the sounds
/// added to the mixer will be converted to these values.
///
/// After creating a mixer, you can add new sounds with the controller.
///
/// Note that mixer without any input source behaves like an `Empty` (not: `Zero`) source,
/// and thus, just after appending to a player, the mixer is removed from the player.
/// As a result, input sources added to the mixer later might not be forwarded to the player.
/// Add `Zero` source to prevent detaching the mixer from player.
pub fn mixer(channels: ChannelCount, sample_rate: SampleRate) -> (Mixer, MixerSource) {
    let (tx, rx) = channel();

    let input = Mixer(Arc::new(Inner {
        pending_tx: tx,
        channels,
        sample_rate,
    }));

    let output = MixerSource {
        current_sources: Vec::new(),
        input: input.clone(),
        current_channel: 0,
        still_pending: Vec::new(),
        pending_rx: rx,
    };

    (input, output)
}

/// The input of the mixer.
#[derive(Clone)]
pub struct Mixer(Arc<Inner>);

struct Inner {
    pending_tx: Sender<Box<dyn Source + Send>>,
    channels: ChannelCount,
    sample_rate: SampleRate,
}

impl Mixer {
    /// Adds a new source to mix to the existing ones.
    #[inline]
    pub fn add<T>(&self, source: T)
    where
        T: Source + Send + 'static,
    {
        let uniform_source =
            UniformSourceIterator::new(source, self.0.channels, self.0.sample_rate);
        // Ignore send errors (channel dropped means MixerSource was dropped)
        let _ = self.0.pending_tx.send(Box::new(uniform_source));
    }

    /// The channel count sources are converted to when added to this mixer.
    pub fn channels(&self) -> ChannelCount {
        self.0.channels
    }

    /// The sample rate sources are converted to when added to this mixer.
    pub fn sample_rate(&self) -> SampleRate {
        self.0.sample_rate
    }

    /// A source in this mixer's format that never produces a sample. Useful as a
    /// [`queue`](crate::queue::queue) keep-alive source, so idling sounds like this one won't
    /// need source conversion set up until real content is appended.
    pub fn silence(&self) -> Empty {
        Empty::new_with_format(self.0.channels, self.0.sample_rate)
    }
}

/// The output of the mixer. Implements `Source`.
pub struct MixerSource {
    // The current iterator that produces samples.
    current_sources: Vec<Box<dyn Source + Send>>,

    // The pending sounds.
    input: Mixer,

    // Current channel position within the frame.
    current_channel: u16,

    // A temporary vec used in start_pending_sources.
    still_pending: Vec<Box<dyn Source + Send>>,

    // Receiver for pending sources from the channel.
    pending_rx: Receiver<Box<dyn Source + Send>>,
}

impl Source for MixerSource {
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.input.0.channels
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.input.0.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    #[inline]
    fn try_seek(&mut self, _: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported {
            underlying_source: std::any::type_name::<Self>(),
        })
    }
}

impl Iterator for MixerSource {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.start_pending_sources();

        let sum = self.sum_current_sources();

        // Advance frame position (wraps at channel count, never overflows)
        self.current_channel += 1;
        if self.current_channel >= self.input.0.channels.get() {
            self.current_channel = 0;
        }

        if self.current_sources.is_empty() {
            None
        } else {
            Some(sum)
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.current_sources.is_empty() {
            return (0, Some(0));
        }

        // The mixer continues as long as ANY source is playing, so bounds are
        // determined by the longest source, not the shortest.
        let mut min = 0;
        let mut max: Option<usize> = Some(0);

        for source in &self.current_sources {
            let (source_min, source_max) = source.size_hint();
            // Lower bound: guaranteed to produce at least until longest source's lower bound
            min = min.max(source_min);

            match (max, source_max) {
                (Some(current_max), Some(source_max_val)) => {
                    // Upper bound: might produce up to longest source's upper bound
                    max = Some(current_max.max(source_max_val));
                }
                _ => {
                    // If any source is unbounded, the mixer is unbounded
                    max = None;
                }
            }
        }

        (min, max)
    }
}

impl MixerSource {
    // Samples from the `next()` function are interlaced for each of the channels.
    // New sources are held in `still_pending` until a frame boundary so their
    // samples stay in-step with the channel layout. Otherwise the sound will
    // play on the wrong channels, e.g. left / right will be reversed.
    fn start_pending_sources(&mut self) {
        while let Ok(source) = self.pending_rx.try_recv() {
            self.still_pending.push(source);
        }

        if self.current_channel == 0 {
            self.current_sources.append(&mut self.still_pending);
        }
    }

    fn sum_current_sources(&mut self) -> Sample {
        let mut sum = 0.0;
        self.current_sources.retain_mut(|source| {
            match source.next() {
                Some(value) => {
                    sum += value;
                    true // Keep this source
                }
                None => false, // Remove exhausted source
            }
        });

        sum
    }
}
