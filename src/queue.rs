//! Queue that plays sounds one after the other.

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use crate::source::Empty;
use crate::source::Source;
use crate::source::Zero;
use crate::Sample;

/// Builds a new queue. It consists of an input and an output.
///
/// The input can be used to add sounds to the end of the queue, while the output implements
/// `Source` and plays the sounds.
///
/// The parameter indicates how the queue should behave if the queue becomes empty:
///
/// - If you pass `true`, then the queue is infinite and will play a silence instead until you add
///   a new sound.
/// - If you pass `false`, then the queue will report that it has finished playing.
///
pub fn queue<S>(keep_alive_if_empty: bool) -> (Arc<SourcesQueueInput<S>>, SourcesQueueOutput<S>)
where
    S: Sample + Send + 'static,
{
    let input = Arc::new(SourcesQueueInput {
        next_sounds: Mutex::new(Vec::new()),
        keep_alive_if_empty: AtomicBool::new(keep_alive_if_empty),
    });

    let output = SourcesQueueOutput {
        current: Box::new(Empty::<S>::new()) as Box<_>,
        signal_after_end: None,
        input: input.clone(),
    };

    (input, output)
}

// TODO: consider reimplementing this with `from_factory`

/// The input of the queue.
pub struct SourcesQueueInput<S> {
    next_sounds: Mutex<Vec<(Box<dyn Source<Item = S> + Send>, Option<Sender<()>>)>>,

    // See constructor.
    keep_alive_if_empty: AtomicBool,
}

impl<S> SourcesQueueInput<S>
where
    S: Sample + Send + 'static,
{
    /// Adds a new source to the end of the queue.
    #[inline]
    pub fn append<T>(&self, source: T)
    where
        T: Source<Item = S> + Send + 'static,
    {
        self.next_sounds
            .lock()
            .unwrap()
            .push((Box::new(source) as Box<_>, None));
    }

    /// Adds a new source to the end of the queue.
    ///
    /// The `Receiver` will be signalled when the sound has finished playing.
    #[inline]
    pub fn append_with_signal<T>(&self, source: T) -> Receiver<()>
    where
        T: Source<Item = S> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        self.next_sounds
            .lock()
            .unwrap()
            .push((Box::new(source) as Box<_>, Some(tx)));
        rx
    }

    /// Sets whether the queue stays alive if there's no more sound to play.
    ///
    /// See also the constructor.
    pub fn set_keep_alive_if_empty(&self, keep_alive_if_empty: bool) {
        self.keep_alive_if_empty
            .store(keep_alive_if_empty, Ordering::Release);
    }
}

/// The output of the queue. Implements `Source`.
pub struct SourcesQueueOutput<S> {
    // The current iterator that produces samples.
    current: Box<dyn Source<Item = S> + Send>,

    // Signal this sender before picking from `next`.
    signal_after_end: Option<Sender<()>>,

    // The next sounds.
    input: Arc<SourcesQueueInput<S>>,
}

impl<S> Source for SourcesQueueOutput<S>
where
    S: Sample + Send + 'static,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        // This function is non-trivial because the boundary between two sounds in the queue should
        // be a frame boundary as well.
        //
        // The current sound is free to return `None` for `current_frame_len()`, in which case
        // we *should* return the number of samples remaining the current sound.
        // This can be estimated with `size_hint()`.
        //
        // If the `size_hint` is `None` as well, we are in the worst case scenario. To handle this
        // situation we force a frame to have a maximum number of samples indicate by this
        // constant.
        const THRESHOLD: usize = 512;

        // Try the current `current_frame_len`.
        if let Some(val) = self.current.current_frame_len() {
            if val != 0 {
                return Some(val);
            }
        }

        // Try the size hint.
        if let Some(val) = self.current.size_hint().1 {
            if val < THRESHOLD && val != 0 {
                return Some(val);
            }
        }

        // Otherwise we use the constant value.
        Some(THRESHOLD)
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.current.channels()
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.current.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl<S> Iterator for SourcesQueueOutput<S>
where
    S: Sample + Send + 'static,
{
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        loop {
            // Basic situation that will happen most of the time.
            if let Some(sample) = self.current.next() {
                return Some(sample);
            }

            // Since `self.current` has finished, we need to pick the next sound.
            // In order to avoid inlining this expensive operation, the code is in another function.
            if self.go_next().is_err() {
                return None;
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.current.size_hint().0, None)
    }
}

impl<S> SourcesQueueOutput<S>
where
    S: Sample + Send + 'static,
{
    // Called when `current` is empty and we must jump to the next element.
    // Returns `Ok` if the sound should continue playing, or an error if it should stop.
    //
    // This method is separate so that it is not inlined.
    fn go_next(&mut self) -> Result<(), ()> {
        if let Some(signal_after_end) = self.signal_after_end.take() {
            let _ = signal_after_end.send(());
        }

        let (next, signal_after_end) = {
            let mut next = self.input.next_sounds.lock().unwrap();

            if next.len() == 0 {
                if self.input.keep_alive_if_empty.load(Ordering::Acquire) {
                    // Play a short silence in order to avoid spinlocking.
                    let silence = Zero::<S>::new(1, 44100); // TODO: meh
                    (
                        Box::new(silence.take_duration(Duration::from_millis(10))) as Box<_>,
                        None,
                    )
                } else {
                    return Err(());
                }
            } else {
                next.remove(0)
            }
        };

        self.current = next;
        self.signal_after_end = signal_after_end;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::buffer::SamplesBuffer;
    use crate::queue;
    use crate::source::Source;

    #[test]
    #[ignore] // FIXME: samples rate and channel not updated immediately after transition
    fn basic() {
        let (tx, mut rx) = queue::queue(false);

        tx.append(SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]));
        tx.append(SamplesBuffer::new(2, 96000, vec![5i16, 5, 5, 5]));

        assert_eq!(rx.channels(), 1);
        assert_eq!(rx.sample_rate(), 48000);
        assert_eq!(rx.next(), Some(10));
        assert_eq!(rx.next(), Some(-10));
        assert_eq!(rx.next(), Some(10));
        assert_eq!(rx.next(), Some(-10));
        assert_eq!(rx.channels(), 2);
        assert_eq!(rx.sample_rate(), 96000);
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), Some(5));
        assert_eq!(rx.next(), None);
    }

    #[test]
    fn immediate_end() {
        let (_, mut rx) = queue::queue::<i16>(false);
        assert_eq!(rx.next(), None);
    }

    #[test]
    fn keep_alive() {
        let (tx, mut rx) = queue::queue(true);
        tx.append(SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]));

        assert_eq!(rx.next(), Some(10));
        assert_eq!(rx.next(), Some(-10));
        assert_eq!(rx.next(), Some(10));
        assert_eq!(rx.next(), Some(-10));

        for _ in 0..100000 {
            assert_eq!(rx.next(), Some(0));
        }
    }

    #[test]
    #[ignore] // TODO: not yet implemented
    fn no_delay_when_added() {
        let (tx, mut rx) = queue::queue(true);

        for _ in 0..500 {
            assert_eq!(rx.next(), Some(0));
        }

        tx.append(SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]));
        assert_eq!(rx.next(), Some(10));
        assert_eq!(rx.next(), Some(-10));
        assert_eq!(rx.next(), Some(10));
        assert_eq!(rx.next(), Some(-10));
    }
}
