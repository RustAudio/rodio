//! Mixer that plays multiple sounds at the same time.

use crate::common::{ChannelCount, SampleRate};
use crate::source::{SeekError, Source, UniformSourceIterator};
use crate::Sample;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Builds a new mixer.
///
/// You can choose the characteristics of the output thanks to this constructor. All the sounds
/// added to the mixer will be converted to these values.
///
/// After creating a mixer, you can add new sounds with the controller.
///
/// Note that mixer without any input source behaves like an `Empty` (not: `Zero`) source,
/// and thus, just after appending to a sink, the mixer is removed from the sink.
/// As a result, input sources added to the mixer later might not be forwarded to the sink.
/// Add `Zero` source to prevent detaching the mixer from sink.
pub fn mixer(channels: ChannelCount, sample_rate: SampleRate) -> (Mixer, MixerSource) {
    let input = Mixer(Arc::new(Inner {
        has_pending: AtomicBool::new(false),
        pending_sources: Mutex::new(Vec::new()),
        channels,
        sample_rate,
    }));

    let output = MixerSource {
        current_sources: Vec::with_capacity(16),
        input: input.clone(),
        sample_count: 0,
        still_pending: vec![],
        still_current: vec![],
    };

    (input, output)
}

/// The input of the mixer.
#[derive(Clone)]
pub struct Mixer(Arc<Inner>);

struct Inner {
    has_pending: AtomicBool,
    pending_sources: Mutex<Vec<Box<dyn Source + Send>>>,
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
        self.0
            .pending_sources
            .lock()
            .unwrap()
            .push(Box::new(uniform_source) as Box<_>);
        self.0.has_pending.store(true, Ordering::SeqCst); // TODO: can we relax this ordering?
    }
}

/// The output of the mixer. Implements `Source`.
pub struct MixerSource {
    // The current iterator that produces samples.
    current_sources: Vec<Box<dyn Source + Send>>,

    // The pending sounds.
    input: Mixer,

    // The number of samples produced so far.
    sample_count: usize,

    // A temporary vec used in start_pending_sources.
    still_pending: Vec<Box<dyn Source + Send>>,

    // A temporary vec used in sum_current_sources.
    still_current: Vec<Box<dyn Source + Send>>,
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

        // uncomment when #510 is implemented (query position of playback)

        // let mut org_positions = Vec::with_capacity(self.current_sources.len());
        // let mut encounterd_err = None;
        //
        // for source in &mut self.current_sources {
        //     let pos = /* source.playback_pos() */ todo!();
        //     if let Err(e) = source.try_seek(pos) {
        //         encounterd_err = Some(e);
        //         break;
        //     } else {
        //         // store pos in case we need to roll back
        //         org_positions.push(pos);
        //     }
        // }
        //
        // if let Some(e) = encounterd_err {
        //     // rollback seeks that happend before err
        //     for (pos, source) in org_positions
        //         .into_iter()
        //         .zip(self.current_sources.iter_mut())
        //     {
        //         source.try_seek(pos)?;
        //     }
        //     Err(e)
        // } else {
        //     Ok(())
        // }
    }
}

impl Iterator for MixerSource {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.input.0.has_pending.load(Ordering::SeqCst) {
            self.start_pending_sources();
        }

        self.sample_count += 1;

        let sum = self.sum_current_sources();

        if self.current_sources.is_empty() {
            None
        } else {
            Some(sum)
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }
}

impl MixerSource {
    // Samples from the #next() function are interlaced for each of the channels.
    // We need to ensure we start playing sources so that their samples are
    // in-step with the modulo of the samples produced so far. Otherwise, the
    // sound will play on the wrong channels, e.g. left / right will be reversed.
    fn start_pending_sources(&mut self) {
        let mut pending = self.input.0.pending_sources.lock().unwrap(); // TODO: relax ordering?

        for source in pending.drain(..) {
            let in_step = self.sample_count % source.channels() as usize == 0;

            if in_step {
                self.current_sources.push(source);
            } else {
                self.still_pending.push(source);
            }
        }
        std::mem::swap(&mut self.still_pending, &mut pending);

        let has_pending = !pending.is_empty();
        self.input
            .0
            .has_pending
            .store(has_pending, Ordering::SeqCst); // TODO: relax ordering?
    }

    fn sum_current_sources(&mut self) -> Sample {
        let mut sum = 0.0;
        for mut source in self.current_sources.drain(..) {
            if let Some(value) = source.next() {
                sum += value;
                self.still_current.push(source);
            }
        }
        std::mem::swap(&mut self.still_current, &mut self.current_sources);

        sum
    }
}

#[cfg(test)]
mod tests {
    use crate::buffer::SamplesBuffer;
    use crate::mixer;
    use crate::source::Source;

    #[test]
    fn basic() {
        let (tx, mut rx) = mixer::mixer(1, 48000);

        tx.add(SamplesBuffer::new(1, 48000, vec![10.0, -10.0, 10.0, -10.0]));
        tx.add(SamplesBuffer::new(1, 48000, vec![5.0, 5.0, 5.0, 5.0]));

        assert_eq!(rx.channels(), 1);
        assert_eq!(rx.sample_rate(), 48000);
        assert_eq!(rx.next(), Some(15.0));
        assert_eq!(rx.next(), Some(-5.0));
        assert_eq!(rx.next(), Some(15.0));
        assert_eq!(rx.next(), Some(-5.0));
        assert_eq!(rx.next(), None);
    }

    #[test]
    fn channels_conv() {
        let (tx, mut rx) = mixer::mixer(2, 48000);

        tx.add(SamplesBuffer::new(1, 48000, vec![10.0, -10.0, 10.0, -10.0]));
        tx.add(SamplesBuffer::new(1, 48000, vec![5.0, 5.0, 5.0, 5.0]));

        assert_eq!(rx.channels(), 2);
        assert_eq!(rx.sample_rate(), 48000);
        assert_eq!(rx.next(), Some(15.0));
        assert_eq!(rx.next(), Some(15.0));
        assert_eq!(rx.next(), Some(-5.0));
        assert_eq!(rx.next(), Some(-5.0));
        assert_eq!(rx.next(), Some(15.0));
        assert_eq!(rx.next(), Some(15.0));
        assert_eq!(rx.next(), Some(-5.0));
        assert_eq!(rx.next(), Some(-5.0));
        assert_eq!(rx.next(), None);
    }

    #[test]
    fn rate_conv() {
        let (tx, mut rx) = mixer::mixer(1, 96000);

        tx.add(SamplesBuffer::new(1, 48000, vec![10.0, -10.0, 10.0, -10.0]));
        tx.add(SamplesBuffer::new(1, 48000, vec![5.0, 5.0, 5.0, 5.0]));

        assert_eq!(rx.channels(), 1);
        assert_eq!(rx.sample_rate(), 96000);
        assert_eq!(rx.next(), Some(15.0));
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), Some(-5.0));
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), Some(15.0));
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), Some(-5.0));
        assert_eq!(rx.next(), None);
    }

    #[test]
    fn start_afterwards() {
        let (tx, mut rx) = mixer::mixer(1, 48000);

        tx.add(SamplesBuffer::new(1, 48000, vec![10.0, -10.0, 10.0, -10.0]));

        assert_eq!(rx.next(), Some(10.0));
        assert_eq!(rx.next(), Some(-10.0));

        tx.add(SamplesBuffer::new(
            1,
            48000,
            vec![5.0, 5.0, 6.0, 6.0, 7.0, 7.0, 7.0],
        ));

        assert_eq!(rx.next(), Some(15.0));
        assert_eq!(rx.next(), Some(-5.0));

        assert_eq!(rx.next(), Some(6.0));
        assert_eq!(rx.next(), Some(6.0));

        tx.add(SamplesBuffer::new(1, 48000, vec![2.0]));

        assert_eq!(rx.next(), Some(9.0));
        assert_eq!(rx.next(), Some(7.0));
        assert_eq!(rx.next(), Some(7.0));

        assert_eq!(rx.next(), None);
    }
}
