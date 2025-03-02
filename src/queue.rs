//! Queue that plays sounds one after the other.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::math::ch;
use crate::source::{Empty, SeekError, Source, Zero};
use crate::source::PeekableSource;
use crate::Sample;

use crate::common::{ChannelCount, SampleRate};

type Sound = Box<dyn Source + Send>;
struct Inner {
    next_sounds: Mutex<Vec<PeekableSource<Sound>>>,
    keep_alive_if_empty: AtomicBool,
}

/// The input of the queue.
pub struct Queue(Arc<Inner>);

impl Queue {
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
    pub fn new(keep_alive_if_empty: bool) -> (Self, QueueSource) {
        let input = Arc::new(Inner {
            next_sounds: Mutex::new(Vec::new()),
            keep_alive_if_empty: AtomicBool::new(keep_alive_if_empty),
        });

        let output = QueueSource {
            current: Empty::new().type_erased().peekable_source(),
            input: input.clone(),
            starting_new_source: false,
        };

        (Self(input), output)
    }

    /// Adds a new source to the end of the queue.
    #[inline]
    pub fn append<T>(&self, source: T)
    where
        T: Source + Send + 'static,
    {
        self.0
            .next_sounds
            .lock()
            .unwrap()
            .push(source.type_erased().peekable_source());
    }

    /// Sets whether the queue stays alive if there's no more sound to play.
    ///
    /// See also the constructor.
    pub fn keep_alive_if_empty(&self, keep_alive_if_empty: bool) {
        self.0 // relaxed: no one depends on the exact order
            .keep_alive_if_empty 
            .store(keep_alive_if_empty, Ordering::Relaxed);
    }

    /// Removes all the sounds from the queue. Returns the number of sounds cleared.
    pub fn clear(&self) -> usize {
        let mut sounds = self.0.next_sounds.lock().unwrap();
        let len = sounds.len();
        sounds.clear();
        len
    }
}

/// Play this source to hear sounds appended to the Queue
pub struct QueueSource {
    current: PeekableSource<Box<dyn Source + Send>>,
    starting_new_source: bool,
    input: Arc<Inner>,
}

const THRESHOLD: usize = 512;

impl Source for QueueSource {
    #[inline]
    fn parameters_changed(&self) -> bool {
        self.current.peek_next().is_none() || self.current.parameters_changed()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.current.channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.current.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    /// Only seeks within the current source.
    // We can not go back to previous sources. We could implement seek such
    // that it advances the queue if the position is beyond the current song.
    //
    // We would then however need to enable seeking backwards across sources too.
    // That no longer seems in line with the queue behavior.
    //
    // A final pain point is that we would need the total duration for the
    // next few songs.
    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.current.try_seek(pos)
    }
}

impl Iterator for QueueSource {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Basic situation that will happen most of the time.
            self.starting_new_source = false;
            if let Some(sample) = self.current.next() {
                return Some(sample);
            }

            // Since `self.current` has finished, we need to pick the next sound.
            // In order to avoid inlining this expensive operation,
            // the code is in another function.
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

impl QueueSource {
    // Called when `current` is empty, and we must jump to the next element.
    // Returns `Ok` if the sound should continue playing, or an error if it should stop.
    //
    // This method is separate so that it is not inlined.
    fn go_next(&mut self) -> Result<(), ()> {
        let next = {
            let mut next = self.input.next_sounds.lock().unwrap();

            if next.is_empty() {
                let silence = Zero::new_samples(ch!(1), 44100, THRESHOLD).type_erased().peekable_source();
                if self.input.keep_alive_if_empty.load(Ordering::Relaxed) {
                    // Play a short silence in order to avoid spinlocking.
                    silence
                } else {
                    return Err(());
                }
            } else {
                next.remove(0)
            }
        };

        self.starting_new_source = true;
        self.current = next;
        Ok(())
    }
}
