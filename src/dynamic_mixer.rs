//! Mixer that plays multiple sounds at the same time.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::source::{Source, UniformSourceIterator};
use crate::Sample;

/// Builds a new mixer.
///
/// You can choose the characteristics of the output thanks to this constructor. All the sounds
/// added to the mixer will be converted to these values.
///
/// After creating a mixer, you can add new sounds with the controller.
pub fn mixer<S>(
    channels: u16,
    sample_rate: u32,
) -> (Arc<DynamicMixerController<S>>, DynamicMixer<S>)
where
    S: Sample + Send + 'static,
{
    let input = Arc::new(DynamicMixerController {
        has_pending: AtomicBool::new(false),
        pending_sources: Mutex::new(Vec::new()),
        channels,
        sample_rate,
    });

    let output = DynamicMixer {
        current_sources: Vec::with_capacity(16),
        input: input.clone(),
        sample_count: 0,
    };

    (input, output)
}

/// The input of the mixer.
pub struct DynamicMixerController<S> {
    has_pending: AtomicBool,
    pending_sources: Mutex<Vec<Box<dyn Source<Item=S> + Send>>>,
    channels: u16,
    sample_rate: u32,
}

impl<S> DynamicMixerController<S>
where
    S: Sample + Send + 'static,
{
    /// Adds a new source to mix to the existing ones.
    #[inline]
    pub fn add<T>(&self, source: T)
    where
        T: Source<Item=S> + Send + 'static,
    {
        let uniform_source = UniformSourceIterator::new(source, self.channels, self.sample_rate);
        let mut pending = self.pending_sources
            .lock()
            .unwrap();
        pending.push(Box::new(uniform_source) as Box<_>);
        self.has_pending.store(true, Ordering::SeqCst); // TODO: can we relax this ordering?
    }
}

/// The output of the mixer. Implements `Source`.
pub struct DynamicMixer<S> {
    // The current iterator that produces samples.
    current_sources: Vec<Box<dyn Source<Item=S> + Send>>,

    // The pending sounds.
    input: Arc<DynamicMixerController<S>>,

    // The number of samples produced so far.
    sample_count: usize,
}

impl<S> Source for DynamicMixer<S>
where
    S: Sample + Send + 'static,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.input.channels
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.input.sample_rate
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl<S> Iterator for DynamicMixer<S>
where
    S: Sample + Send + 'static,
{
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        if self.input.has_pending.load(Ordering::SeqCst) {
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

impl<S> DynamicMixer<S>
where
    S: Sample + Send + 'static,
{
    // Samples from the #next() function are interlaced for each of the channels.
    // We need to ensure we start playing sources so that their samples are
    // in-step with the modulo of the samples produced so far. Otherwise, the
    // sound will play on the wrong channels, e.g. left / right will be reversed.
    fn start_pending_sources(&mut self) {
        let mut pending = self.input.pending_sources.lock().unwrap();
        let mut i = 0;
        while i < pending.len() {
            let in_step = self.sample_count % pending[i].channels() as usize == 0;
            if in_step {
                self.current_sources.push(pending.swap_remove(i));
            } else {
                i += 1;
            }
        }

        let has_pending = !pending.is_empty();
        self.input.has_pending.store(has_pending, Ordering::SeqCst); // TODO: relax ordering?
    }

    fn sum_current_sources(&mut self) -> S {
        let mut sum = S::zero_value();
        let mut i = 0;
        while i < self.current_sources.len() {
            if let Some(value) = self.current_sources[i].next() {
                sum = sum.saturating_add(value);
                i += 1;
            } else {
                self.current_sources.swap_remove(i);
            }
        }
        sum
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::ops::Deref;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU8};
    use std::sync::atomic::Ordering::{Acquire, Release};
    use crate::buffer::SamplesBuffer;
    use crate::dynamic_mixer;
    use crate::source::Source;

    #[test]
    pub fn fff() {
        let r = Arc::new(AtomicU8::new(12));
        let c = r.clone();
        r.store(44, Release);
        assert_eq!(r.load(Acquire), c.load(Acquire));
    }

    #[test]
    fn basic() {
        let (tx, mut rx) = dynamic_mixer::mixer(1, 48000);

        tx.add(SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]));
        tx.add(SamplesBuffer::new(1, 48000, vec![5i16, 5, 5, 5]));

        assert_eq!(rx.channels(), 1);
        assert_eq!(rx.sample_rate(), 48000);
        assert_eq!(rx.next(), Some(15));
        assert_eq!(rx.next(), Some(-5));
        assert_eq!(rx.next(), Some(15));
        assert_eq!(rx.next(), Some(-5));
        assert_eq!(rx.next(), None);
    }

    #[test]
    fn channels_conv() {
        let (tx, mut rx) = dynamic_mixer::mixer(2, 48000);

        tx.add(SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]));
        tx.add(SamplesBuffer::new(1, 48000, vec![5i16, 5, 5, 5]));

        assert_eq!(rx.channels(), 2);
        assert_eq!(rx.sample_rate(), 48000);
        assert_eq!(rx.next(), Some(15));
        assert_eq!(rx.next(), Some(15));
        assert_eq!(rx.next(), Some(-5));
        assert_eq!(rx.next(), Some(-5));
        assert_eq!(rx.next(), Some(15));
        assert_eq!(rx.next(), Some(15));
        assert_eq!(rx.next(), Some(-5));
        assert_eq!(rx.next(), Some(-5));
        assert_eq!(rx.next(), None);
    }

    #[test]
    fn rate_conv() {
        let (tx, mut rx) = dynamic_mixer::mixer(1, 96000);

        tx.add(SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]));
        tx.add(SamplesBuffer::new(1, 48000, vec![5i16, 5, 5, 5]));

        assert_eq!(rx.channels(), 1);
        assert_eq!(rx.sample_rate(), 96000);
        assert_eq!(rx.next(), Some(15));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(-5));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(15));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(-5));
        assert_eq!(rx.next(), None);
    }

    #[test]
    fn start_afterwards() {
        let (tx, mut rx) = dynamic_mixer::mixer(1, 48000);

        tx.add(SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]));

        assert_eq!(rx.next(), Some(10));
        assert_eq!(rx.next(), Some(-10));

        tx.add(SamplesBuffer::new(1, 48000, vec![5i16, 5, 6, 6, 7, 7, 7]));

        assert_eq!(rx.next(), Some(15));
        assert_eq!(rx.next(), Some(-5));

        assert_eq!(rx.next(), Some(6));
        assert_eq!(rx.next(), Some(6));

        tx.add(SamplesBuffer::new(1, 48000, vec![2i16]));

        assert_eq!(rx.next(), Some(9));
        assert_eq!(rx.next(), Some(7));
        assert_eq!(rx.next(), Some(7));

        assert_eq!(rx.next(), None);
    }
}
