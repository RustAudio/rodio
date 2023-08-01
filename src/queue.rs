//! Queue that plays sounds one after the other.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::source::{Empty, Source, Zero};
use crate::Sample;

#[cfg(feature = "crossbeam-channel")]
use crossbeam_channel::{unbounded as channel, Receiver, Sender};
#[cfg(not(feature = "crossbeam-channel"))]
use std::sync::mpsc::{channel, Receiver, Sender};

type BoxedSource<S> = Box<dyn Source<Item = S> + Send>;
type QueueNextItem<S> = (BoxedSource<S>, Option<Sender<()>>);

trait InputQueue<S> {
    fn keep_alive_if_empty(&self) -> bool;
    fn has_next(&self) -> bool;
    fn next(&self) -> Option<QueueNextItem<S>>;
}

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

pub fn id_queue<S, I>(keep_alive_if_empty: bool) -> (Arc<SourcesIdQueueInput<S, I>>, SourcesQueueOutput<S>)
where
  S: Sample + Send + 'static,
  I: Eq + PartialEq + Send + 'static,
{
    let input = Arc::new(SourcesIdQueueInput {
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
    next_sounds: Mutex<Vec<QueueNextItem<S>>>,

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
    ///
    /// Enable the feature flag `crossbeam-channel` in rodio to use a `crossbeam_channel::Receiver` instead.
    #[inline]
    pub fn append_with_signal<T>(&self, source: T) -> Receiver<()>
    where
        T: Source<Item = S> + Send + 'static,
    {
        let (tx, rx) = channel();
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

    /// Removes all the sounds from the queue. Returns the number of sounds cleared.
    pub fn clear(&self) -> usize {
        let mut sounds = self.next_sounds.lock().unwrap();
        let len = sounds.len();
        sounds.clear();
        len
    }
}

impl<S> InputQueue<S> for SourcesQueueInput<S>
where
    S: Sample + Send + 'static,
{
    fn keep_alive_if_empty(&self) -> bool {
        self.keep_alive_if_empty.load(Ordering::Acquire)
    }
    
    fn has_next(&self) -> bool {
        self.next_sounds.lock().unwrap().len() > 0
    }

    fn next(&self) -> Option<QueueNextItem<S>> {
        let mut next = self.next_sounds.lock().unwrap();
        if next.len() > 0 {
            Some(next.remove(0))
        } else {
            None
        }
    }
}

/// A queue input that can associate ids with each item. This allows the queue to be used more like
/// a playlist where items can be removed and/or reordered
pub struct SourcesIdQueueInput<S, I> {
    next_sounds: Mutex<Vec<(I, QueueNextItem<S>)>>,

    // See constructor.
    keep_alive_if_empty: AtomicBool,
}

impl<S, I> SourcesIdQueueInput<S, I>
where
    S: Sample + Send + 'static,
    I: Eq + PartialEq,
{
    /// Adds a new source to the end of the queue.
    #[inline]
    pub fn append<T>(&self, id: I, source: T)
    where
        T: Source<Item = S> + Send + 'static,
    {
        self.next_sounds
            .lock()
            .unwrap()
            .push((id, (Box::new(source) as Box<_>, None)));
    }

    /// Adds a new source to the end of the queue.
    ///
    /// The `Receiver` will be signalled when the sound has finished playing.
    ///
    /// Enable the feature flag `crossbeam-channel` in rodio to use a `crossbeam_channel::Receiver` instead.
    #[inline]
    pub fn append_with_signal<T>(&self, id: I, source: T) -> Receiver<()>
    where
        T: Source<Item = S> + Send + 'static,
    {
        let (tx, rx) = channel();
        self.next_sounds
            .lock()
            .unwrap()
            .push((id, (Box::new(source) as Box<_>, Some(tx))));
        rx
    }

    /// Remove the item with id `id` from the queue
    #[inline]
    pub fn remove(&self, id: I) {
        let mut next = self.next_sounds.lock().unwrap();
        next.retain(|i| i.0 != id);
    }

    /// Swap item having id `id_a` with the item having id `id_b`. If either item does not exist,
    /// this is a no-op
    pub fn swap(&self, id_a: I, id_b: I) {
        let mut next_sounds = self.next_sounds.lock().unwrap();
        let mut index_a = None;
        let mut index_b = None;
        let mut p = 0;
        for (id, _) in next_sounds.iter() {
            if index_a.is_none() && *id == id_a {
                index_a = Some(p);
            } else if index_b.is_none() && *id == id_b {
                index_b = Some(p);
            }
            p+=1;
        }
        if let (Some(index_a), Some(index_b)) = (index_a, index_b) {
            next_sounds.swap(index_a, index_b);
        }
    }
}

impl<S, I> InputQueue<S> for SourcesIdQueueInput<S, I>
where
    S: Sample + Send + 'static,
{
    fn keep_alive_if_empty(&self) -> bool {
        self.keep_alive_if_empty.load(Ordering::Acquire)
    }
    
    fn has_next(&self) -> bool {
        self.next_sounds.lock().unwrap().len() > 0
    }

    fn next(&self) -> Option<QueueNextItem<S>> {
        let mut next = self.next_sounds.lock().unwrap();
        if next.len() > 0 {
            Some(next.remove(0).1)
        } else {
            None
        }
    }
}

/// The output of the queue. Implements `Source`.
pub struct SourcesQueueOutput<S> {
    // The current iterator that produces samples.
    current: Box<dyn Source<Item = S> + Send>,

    // Signal this sender before picking from `next`.
    signal_after_end: Option<Sender<()>>,

    // The next sounds.
    input: Arc<dyn InputQueue<S> + Send + Sync>,
}

const THRESHOLD: usize = 512;
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

        // Try the current `current_frame_len`.
        if let Some(val) = self.current.current_frame_len() {
            if val != 0 {
                return Some(val);
            } else if self.input.keep_alive_if_empty() && self.input.has_next() {
                // The next source will be a filler silence which will have the length of `THRESHOLD`
                return Some(THRESHOLD);
            }
        }

        // Try the size hint.
        let (lower_bound, _) = self.current.size_hint();
        // The iterator default implementation just returns 0.
        // That's a problematic value, so skip it.
        if lower_bound > 0 {
            return Some(lower_bound);
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

        let (next, signal_after_end) = match self.input.next() {
            Some(t) => t,
            None => {
                let silence = Box::new(Zero::<S>::new_samples(1, 44100, THRESHOLD)) as Box<_>;
                if self.input.keep_alive_if_empty() {
                    // Play a short silence in order to avoid spinlocking.
                    (silence, None)
                } else {
                    return Err(());
                }
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
    fn simple_id() {
        let (tx, mut rx) = queue::id_queue(false);
        let s1 = "sb1".to_string();
        let s2 = "sb2".to_string();
        let v1 = vec![10i16, -10, 10, -10];
        let v2 = vec![10i16, -9, 9, -9];
        tx.append(s1, SamplesBuffer::new(1, 48000, v1.clone()));
        tx.append(s2, SamplesBuffer::new(1, 48000, v2.clone()));
        assert_eq!(rx.channels(), 1);
        assert_eq!(rx.sample_rate(), 48000);
        for v in v1.into_iter().chain(v2.into_iter()) {
            assert_eq!(rx.next(), Some(v));
        }
    }

    #[test]
    fn id_with_remove() {
        let (tx, mut rx) = queue::id_queue(false);
        let s1 = "sb1".to_string();
        let s2 = "sb2".to_string();
        tx.append(s1, SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]));
        tx.append(s2, SamplesBuffer::new(1, 48000, vec![10i16, -9, 9, -9]));
        assert_eq!(rx.channels(), 1);
        assert_eq!(rx.sample_rate(), 48000);
        assert_eq!(rx.next(), Some(10));
        assert_eq!(rx.next(), Some(-10));
        tx.remove("sb2".to_string());
        assert_eq!(rx.next(), Some(10));
        assert_eq!(rx.next(), Some(-10));
        assert_eq!(rx.next(), None);
    }

    #[test]
    fn id_swap() {
        let (tx, mut rx) = queue::id_queue(false);
        let s1 = "sb1".to_string();
        let s2 = "sb2".to_string();
        let s3 = "sb3".to_string();
        let v1 = vec![10i16, -10, 10, -10];
        let v2 = vec![10i16, -9, 9, -9];
        let v3 = vec![12i16, -12, 12, -12];
        tx.append(s1, SamplesBuffer::new(1, 48000, v1.clone()));
        tx.append(s2.clone(), SamplesBuffer::new(1, 48000, v2.clone()));
        tx.append(s3.clone(), SamplesBuffer::new(1, 48000, v3.clone()));
        assert_eq!(rx.channels(), 1);
        assert_eq!(rx.sample_rate(), 48000);
        tx.swap(s3, s2);
        for v in v1.into_iter().chain(v3.into_iter()).chain(v2.into_iter()) {
            assert_eq!(rx.next(), Some(v));
        }
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
