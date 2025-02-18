//! Queue that plays sounds one after the other.

use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::source::{Empty, SeekError, Source, Zero};
use crate::Sample;

use crate::common::{ChannelCount, SampleRate};

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
pub fn queue<S>(keep_alive_if_empty: bool) -> (Arc<QueueControls<S>>, QueueSource<S>)
where
    S: Sample + Send + 'static,
{
    let input = Arc::new(QueueControls {
        next_sounds: Mutex::new(VecDeque::new()),
        keep_alive_if_empty: AtomicBool::new(keep_alive_if_empty),
    });

    let output = QueueSource {
        current: Box::new(Empty::<S>::new()) as Box<_>,
        input: input.clone(),
        starting_silence: Cell::new(false),
        buffered: None,
        samples_left_in_span: Cell::new(0),
        starting_silence_channels: Cell::new(2),
        starting_silence_sample_rate: Cell::new(4100),
    };

    (input, output)
}

type Sound<S> = Box<dyn Source<Item = S> + Send>;
/// The input of the queue.
pub struct QueueControls<S> {
    next_sounds: Mutex<VecDeque<Sound<S>>>,

    // See constructor.
    keep_alive_if_empty: AtomicBool,
}

impl<S> QueueControls<S>
where
    S: Sample + Send + 'static,
{
    /// Adds a new source to the end of the queue.
    ///
    /// If silence was playing it can take up to <TODO> milliseconds before
    /// the new sound is played.
    ///
    /// Sources of only one sample are skipped (though next is still called on them).
    #[inline]
    pub fn append<T>(&self, source: T)
    where
        T: Source<Item = S> + Send + 'static,
    {
        self.next_sounds
            .lock()
            .unwrap()
            .push_back(Box::new(source) as Box<_>);
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
/// The output of the queue. Implements `Source`.
pub struct QueueSource<S> {
    // The current iterator that produces samples.
    current: Box<dyn Source<Item = S> + Send>,

    // The next sounds.
    input: Arc<QueueControls<S>>,

    starting_silence: Cell<bool>,
    starting_silence_channels: Cell<ChannelCount>,
    starting_silence_sample_rate: Cell<SampleRate>,

    samples_left_in_span: Cell<usize>,

    buffered: Option<S>,
}

impl<S> Source for QueueSource<S>
where
    S: Sample + Send + 'static + core::fmt::Debug,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        // This function is non-trivial because the boundary between two
        // sounds in the queue should be a span boundary as well. Further more
        // we can *only* return Some(0) if the queue should stop playing.
        //
        // This function can be called at any time though its normally only
        // called at the end of the span to get how long the next span will be.
        //
        // The current sound is free to return `None` for
        // `current_span_len()`. That means there is only one span left and it
        // lasts until the end of the sound. We get a lower bound on that
        // length using `size_hint()`.
        //
        // If the `size_hint` is `None` as well, we are in the worst case
        // scenario. To handle this situation we force a span to have a
        // maximum number of samples with a constant. If the source ends before
        // that point we need to start silence for the remainder of the forced span.

        let (span_len, size_lower_bound) = if self.buffered.is_none() {
            if let Some(next) = self.next_non_empty_sound_params() {
                (next.span_len, next.size_lower_bound)
            } else if self.should_end_when_input_empty() {
                return Some(0);
            } else {
                self.starting_silence.set(true);
                return Some(self.silence_span_len());
            }
        } else {
            (self.current.current_span_len(), self.current.size_hint().0)
        };

        if self.samples_left_in_span.get() > 0 {
            return Some(self.samples_left_in_span.get());
        }

        let res = if let Some(len) = span_len {
            // correct len for buffered sample
            let len = if self.buffered.is_some() {
                len + 1
            } else {
                len
            };

            if len > 0 {
                Some(len)
            } else if self.should_end_when_input_empty() {
                Some(0)
            } else {
                // Must be first call after creation with nothing pushed yet.
                // Call to next should be silence. A source pushed between this call
                // and the first call to next could cause a bug here.
                //
                // We signal to next that we need a silence now even if a new
                // source is available
                self.starting_silence.set(true);
                if let Some(params) = self.next_non_empty_sound_params() {
                    self.starting_silence_sample_rate.set(params.sample_rate);
                    self.starting_silence_channels.set(params.channels);
                } else {
                    self.starting_silence_sample_rate.set(44_100);
                    self.starting_silence_channels.set(2);
                };
                Some(self.silence_span_len())
            }
        } else if size_lower_bound == 0 {
            // span could end earlier we correct for that by playing silence
            // if that happens
            Some(self.fallback_span_length())
        } else {
            Some(size_lower_bound)
        };

        if let Some(len) = res {
            self.samples_left_in_span.set(len);
        }

        res
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        if self.buffered.is_none() {
            if let Some(next) = self.next_non_empty_sound_params() {
                next.channels
            } else {
                self.starting_silence_channels.set(2);
                2
            }
        } else {
            self.current.channels()
        }
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        if self.buffered.is_none() {
            if let Some(next) = self.next_non_empty_sound_params() {
                next.sample_rate
            } else {
                self.starting_silence_sample_rate.set(44_100);
                44100
            }
        } else {
            self.current.sample_rate()
        }
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

impl<S> Iterator for QueueSource<S>
where
    S: Sample + Send + 'static + std::fmt::Debug,
{
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        // may only return None when the queue should end
        let res = match dbg!((self.buffered.take(), self.current.next())) {
            (Some(sample1), Some(samples2)) => {
                self.buffered = Some(samples2);
                Some(sample1)
            }
            (Some(sample1), None) => self.current_is_ending(sample1),
            (None, Some(sample1)) => {
                // start, populate the buffer
                self.buffered = self.current.next();
                Some(sample1)
            }
            (None, None) => self.no_buffer_no_source(),
        };

        if let Some(samples_left) = self.samples_left_in_span.get().checked_sub(1) {
            self.samples_left_in_span.set(dbg!(samples_left));
        }

        res
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }
}

impl<S> QueueSource<S>
where
    S: Sample + Send + 'static + core::fmt::Debug,
{
    fn fallback_span_length(&self) -> usize {
        // ~ 5 milliseconds at 44100
        200 * self.channels() as usize
    }

    fn finish_span_with_silence(&self, samples: usize) -> Sound<S> {
        let silence =
            Zero::<S>::new_samples(self.current.channels(), self.current.sample_rate(), samples);
        Box::new(silence)
    }

    fn silence_span_len(&self) -> usize {
        // ~ 5 milliseconds at 44100
        200 * self.channels() as usize
    }

    fn silence(&self) -> Sound<S> {
        let samples = self.silence_span_len();
        // silence matches span params to make sure resampling
        // gives not popping. It also makes the queue code simpler
        let silence =
            Zero::<S>::new_samples(self.current.channels(), self.current.sample_rate(), samples);
        Box::new(silence)
    }

    fn should_end_when_input_empty(&self) -> bool {
        !self.input.keep_alive_if_empty.load(Ordering::Acquire)
    }

    fn next_sound(&self) -> Option<Sound<S>> {
        self.input.next_sounds.lock().unwrap().pop_front()
    }

    fn no_buffer_no_source(&mut self) -> Option<S> {
        // Prevents a race condition where a call `current_span_len`
        // precedes the call to `next`
        if dbg!(self.starting_silence.get()) {
            self.current = self.silence();
            self.starting_silence.set(true);
            return self.current.next();
        }

        loop {
            if let Some(mut sound) = self.next_sound() {
                if let Some((sample1, sample2)) = sound.next().zip(sound.next()) {
                    self.current = sound;
                    self.buffered = Some(sample2);
                    self.current_span_len();
                    return Some(sample1);
                } else {
                    continue;
                }
            } else if self.should_end_when_input_empty() {
                return None;
            } else {
                self.current = self.silence();
                return self.current.next();
            }
        }
    }

    fn current_is_ending(&mut self, sample1: S) -> Option<S> {
        // note sources are free to stop (return None) mid frame and
        // mid span, we must handle that here

        // check if the span we reported is ended after returning the
        // buffered source. If not we need to provide a silence to guarantee
        // the span ends when we promised
        if self.samples_left_in_span.get() > 1 {
            dbg!(&self.samples_left_in_span);
            self.current = self.finish_span_with_silence(self.samples_left_in_span.get() - 1);
            return Some(sample1);
        }

        loop {
            if let Some(mut sound) = self.next_sound() {
                if let Some(sample2) = sound.next() {
                    self.current = sound;
                    // updates samples_left_in_span
                    self.buffered = Some(sample2);
                    self.current_span_len();
                    return Some(sample1);
                } else {
                    continue;
                }
            } else if self.should_end_when_input_empty() {
                return Some(sample1);
            } else {
                self.current = self.silence();
                self.current_span_len();
                self.buffered = self.current.next();
                return Some(sample1);
            }
        }
    }

    fn next_non_empty_sound_params(&self) -> Option<NonEmptySourceParams> {
        let next_sounds = self.input.next_sounds.lock().unwrap();
        next_sounds
            .iter()
            .find(|s| s.current_span_len().is_none_or(|l| l > 0))
            .map(|s| NonEmptySourceParams {
                size_lower_bound: s.size_hint().0,
                span_len: s.current_span_len(),
                channels: s.channels(),
                sample_rate: s.sample_rate(),
            })
    }
}

#[derive(Debug)]
struct NonEmptySourceParams {
    size_lower_bound: usize,
    span_len: Option<usize>,
    channels: ChannelCount,
    sample_rate: SampleRate,
}
