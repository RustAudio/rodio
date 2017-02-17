//! Queue that plays sounds one after the other.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use source::Source;
use source::UniformSourceIterator;

use Sample;

/// Builds a new mixer.
///
/// You can choose the characteristics of the output thanks to this constructor. All the sounds
/// added to the mixer will be converted to these values.
///
/// After creating a mixer, you can add new sounds with the controller.
pub fn mixer<S>(channels: u16, samples_rate: u32) -> (Arc<DynamicMixerController<S>>, DynamicMixer<S>)
    where S: Sample + Send + 'static
{
    let input = Arc::new(DynamicMixerController {
        has_pending: AtomicBool::new(false),
        pending_sources: Mutex::new(Vec::new()),
        channels: channels,
        samples_rate: samples_rate,
    });

    let output = DynamicMixer {
        current_sources: Vec::with_capacity(16),
        input: input.clone(),
    };

    (input, output)
}

/// The input of the mixer.
pub struct DynamicMixerController<S> {
    has_pending: AtomicBool,
    pending_sources: Mutex<Vec<Box<Source<Item = S> + Send>>>,
    channels: u16,
    samples_rate: u32,
}

impl<S> DynamicMixerController<S> where S: Sample + Send + 'static {
    /// Adds a new source to mix to the existing ones.
    #[inline]
    pub fn add<T>(&self, source: T)
        where T: Source<Item = S> + Send + 'static
    {
        let uniform_source = UniformSourceIterator::new(source, self.channels, self.samples_rate);
        self.pending_sources.lock().unwrap().push(Box::new(uniform_source) as Box<_>);
        self.has_pending.store(true, Ordering::SeqCst);     // TODO: can we relax this ordering?
    }
}

/// The output of the mixer. Implements `Source`.
pub struct DynamicMixer<S> {
    // The current iterator that produces samples.
    current_sources: Vec<Box<Source<Item = S> + Send>>,

    // The pending sounds.
    input: Arc<DynamicMixerController<S>>,
}

impl<S> Source for DynamicMixer<S> where S: Sample + Send + 'static {
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        self.input.channels
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        self.input.samples_rate
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        None
    }
}

impl<S> Iterator for DynamicMixer<S> where S: Sample + Send + 'static {
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        if self.input.has_pending.load(Ordering::SeqCst) {      // TODO: relax ordering?
            let mut pending = self.input.pending_sources.lock().unwrap();
            self.current_sources.extend(pending.drain(..));
            self.input.has_pending.store(false, Ordering::SeqCst);      // TODO: relax ordering?
        }

        if self.current_sources.is_empty() {
            return None;
        }

        let mut to_drop = Vec::new();

        let mut sum = S::zero_value();
        for (num, src) in self.current_sources.iter_mut().enumerate() {
            if let Some(val) = src.next() {
                sum += val;
            } else {
                to_drop.push(num);
            }
        }

        for &td in to_drop.iter().rev() {
            self.current_sources.remove(td);
        }

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

#[cfg(test)]
mod tests {
    use dynamic_mixer;
    use buffer::SamplesBuffer;
    use source::Source;

    #[test]
    fn basic() {
        let (tx, mut rx) = dynamic_mixer::mixer(1, 48000);

        tx.add(SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]));
        tx.add(SamplesBuffer::new(1, 48000, vec![5i16, 5, 5, 5]));

        assert_eq!(rx.get_channels(), 1);
        assert_eq!(rx.get_samples_rate(), 48000);
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

        assert_eq!(rx.get_channels(), 2);
        assert_eq!(rx.get_samples_rate(), 48000);
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

        assert_eq!(rx.get_channels(), 1);
        assert_eq!(rx.get_samples_rate(), 96000);
        assert_eq!(rx.next(), Some(15));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(-5));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(15));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(-5));
        assert_eq!(rx.next(), None);
    }
}
