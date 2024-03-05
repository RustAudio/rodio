//! Mixer that plays multiple sounds at the same time.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::source::{Source, UniformSourceIterator};
use crate::Sample;

type DynSource<S> = Box<dyn Source<Item = S> + Send + Sync + 'static>;

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
    S: Sample + Send + Sync + 'static,
{
    let input = Arc::new(DynamicMixerController {
        has_pending: AtomicBool::new(false),
        pending_sources: Mutex::new(Vec::new()),
        channels,
        sample_rate,
    });

    let output = input.clone().into();

    (input, output)
}

/// A mixer that can be used as the global target of a sink. This should be used with `MixerDriver` and `MixerController`.
///
/// Note that this does _not_ require a `Source` implementation - as the inputs are determined by `MixerDriver`, the
/// mixer itself cannot specify e.g. frame length or total duration.
// TODO: Should we allow mixers to specify frame lenght,
// TODO: Add an "advanced" mixer interface and implement it for any `T: Mixer`, so that things like the
//       pending sources queue can be configured - e.g. for single-threaded mixers.
pub trait Mixer: Iterator {
    /// Adds a new source to mix to the existing ones.
    fn drain_sources(
        &mut self,
        sample_count: usize,
        sources: &mut Vec<DynSource<Self::Item>>,
    );
}

/// The input of the mixer.
pub struct DynamicMixerController<S> {
    has_pending: AtomicBool,
    // TODO: Make this configurable - we can probably use a lockfree queue
    pending_sources: Mutex<Vec<DynSource<S>>>,
    channels: u16,
    sample_rate: u32,
}

impl<S> DynamicMixerController<S>
where
    S: Sample + Send + Sync + 'static,
{
    /// Adds a new source to mix to the existing ones.
    #[inline]
    pub fn add<T>(&self, source: T)
    where
        T: Source<Item = S> + Send + Sync + 'static,
    {
        let uniform_source = UniformSourceIterator::new(source, self.channels, self.sample_rate);
        self.pending_sources
            .lock()
            .unwrap()
            .push(Box::new(uniform_source) as _);
        self.has_pending.store(true, Ordering::SeqCst); // TODO: can we relax this ordering?
    }
}

pub type DynamicMixer<S> = MixerDriver<S, BasicMixer<S>>;

/// A basic summing mixer.
pub struct BasicMixer<S> {
    // The current iterator that produces samples.
    current_sources: Vec<DynSource<S>>,

    // A temporary vec used in start_pending_sources.
    still_pending: Vec<DynSource<S>>,

    // A temporary vec used in sum_current_sources.
    still_current: Vec<DynSource<S>>,
}

impl<S> Default for BasicMixer<S> {
    fn default() -> Self {
        Self {
            current_sources: Vec::with_capacity(16),
            still_pending: vec![],
            still_current: vec![],
        }
    }
}

/// The output of the mixer. Implements `Source`.
pub struct MixerDriver<S, M: ?Sized> {
    // The pending sounds.
    // TODO: Should this be `Weak`, since a new controller cannot be created once the
    //       original one has been dropped.
    input: Arc<DynamicMixerController<S>>,

    // The number of samples produced so far.
    sample_count: usize,

    mixer: M,
}

impl<S, M> MixerDriver<S, M> {
    /// Create a new mixer driver, given an existing mixer.
    ///
    /// If the mixer implements `Default`, you can use `MixerDriver::new`.
    pub fn from_mixer(
        mixer: M,
        channels: u16,
        sample_rate: u32,
    ) -> (Arc<DynamicMixerController<S>>, Self) {
        let controller = Arc::new(DynamicMixerController {
            has_pending: AtomicBool::new(false),
            pending_sources: Mutex::new(Vec::new()),
            channels,
            sample_rate,
        });

        let mixer = Self {
            mixer,
            input: controller.clone(),
            sample_count: 0,
        };

        (controller, mixer)
    }
}

impl<S, M> MixerDriver<S, M>
where
    M: Default,
{
    /// Create a new mixer driver, along with a controller to control the mixer by.
    pub fn new(channels: u16, sample_rate: u32) -> (Arc<DynamicMixerController<S>>, Self) {
        Self::from_mixer(Default::default(), channels, sample_rate)
    }
}

impl<S, M> From<Arc<DynamicMixerController<S>>> for MixerDriver<S, M>
where
    M: Default,
{
    fn from(value: Arc<DynamicMixerController<S>>) -> Self {
        Self {
            mixer: Default::default(),
            input: value,
            sample_count: 0,
        }
    }
}

impl<M> Iterator for MixerDriver<M::Item, M>
where
    M: Mixer + ?Sized,
{
    type Item = M::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.input.has_pending.load(Ordering::SeqCst) {
            let mut pending = self.input.pending_sources.lock().unwrap(); // TODO: relax ordering?
            self.mixer.drain_sources(self.sample_count, &mut *pending);
            let has_pending = !pending.is_empty();
            self.input.has_pending.store(has_pending, Ordering::SeqCst); // TODO: relax ordering?
        }

        self.sample_count += 1;

        self.mixer.next()
    }
}

impl<M> Source for MixerDriver<M::Item, M>
where
    M: Mixer + ?Sized,
    M::Item: Sample,
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

impl<S> Iterator for BasicMixer<S>
where
    S: Sample + Send + Sync + 'static,
{
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
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

impl<S> Mixer for BasicMixer<S>
where
    S: Sample + Send + Sync + 'static,
{
    // Samples from the #next() function are interlaced for each of the channels.
    // We need to ensure we start playing sources so that their samples are
    // in-step with the modulo of the samples produced so far. Otherwise, the
    // sound will play on the wrong channels, e.g. left / right will be reversed.
    fn drain_sources(
        &mut self,
        sample_count: usize,
        pending: &mut Vec<DynSource<S>>,
    ) {
        for source in pending.drain(..) {
            let in_step = sample_count % source.channels() as usize == 0;

            if in_step {
                self.current_sources.push(source);
            } else {
                self.still_pending.push(source);
            }
        }
        std::mem::swap(&mut self.still_pending, pending);
    }
}

impl<S> BasicMixer<S>
where
    S: Sample + Send + Sync + 'static,
{
    fn sum_current_sources(&mut self) -> S {
        let mut sum = S::zero_value();

        for mut source in self.current_sources.drain(..) {
            if let Some(value) = source.next() {
                sum = sum.saturating_add(value);
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
    use crate::dynamic_mixer;
    use crate::source::Source;

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
