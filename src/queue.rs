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
        filling_initial_silence: Cell::new(false),
        next_sample: None,
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

    filling_initial_silence: Cell<bool>,

    next_sample: Option<S>,
}

impl<S> Source for QueueSource<S>
where
    S: Sample + Send + 'static,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        // This function is non-trivial because the boundary between two
        // sounds in the queue should be a span boundary as well. Further more
        // we can *only* return Some(0) if the queue should stop playing.
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
        // maximum number of samples with a constant.
        //
        // There are a lot of cases here:
        // - not filling silence, current span is done
        //     move to next
        // - not filling silence, known span length.
        //     report span length from current
        // - not filling silence, unknown span length have lower bound.
        //     report lower bound
        // - not filling silence, unknown span length, no lower bound.
        //     report fixed number of frames, if its too long we will get
        //     silence for that length
        // - filling silence, we have a next, however span is not finished,
        //   next is same channel count and sample rate.
        //     move to next,
        // - filling silence, we have a next, however span is not finished,
        //   next is diff channel count or sample rate.
        //     play silence for rest of span
        // - filling silence, we have a next, span is done
        //     move to next
        // - filling silence, no next, however span is not finished.
        //     return samples left in span
        // - filling silence, no next, span is done.
        //     new silence span with fixed length, match previous sample_rate
        //     and channel count.

        if let Some(len) = self.current.current_span_len() {
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
                self.filling_initial_silence.set(true);
                Some(self.silence_span_len())
            }
        } else if self.current.size_hint().0 == 0 {
            // This is still an issue, span could end earlier
            // we *could* correct for that by playing silence if that happens
            // but that gets really involved.
            Some(self.fallback_span_length())
        } else {
            Some(self.current.size_hint().0)
        }
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

impl<S> Iterator for QueueSource<S>
where
    S: Sample + Send + 'static,
{
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        // may only return None when the queue should end
        match (self.next_sample.take(), self.current.next()) {
            (Some(sample1), Some(samples2)) => {
                self.next_sample = Some(samples2);
                Some(sample1)
            }
            (Some(sample1), None) => self.current_is_ending(sample1),
            (None, Some(sample1)) => {
                // start, populate the buffer
                self.next_sample = self.current.next();
                Some(sample1)
            }
            (None, None) => self.no_buffer_no_source(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.current.size_hint().0, None)
    }
}

impl<S> QueueSource<S>
where
    S: Sample + Send + 'static,
{
    fn fallback_span_length(&self) -> usize {
        200 * self.channels() as usize
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
        if self.filling_initial_silence.get() {
            self.current = self.silence();
            self.filling_initial_silence.set(true);
            return self.current.next();
        }

        loop {
            if let Some(mut sound) = self.next_sound() {
                if let Some((sample1, sample2)) = sound.next().zip(sound.next()) {
                    self.current = sound;
                    self.next_sample = Some(sample2);
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
        loop {
            if let Some(mut sound) = self.next_sound() {
                if let Some(sample2) = sound.next() {
                    self.current = sound;
                    self.next_sample = Some(sample2);
                    return Some(sample1);
                } else {
                    continue;
                }
            } else if self.should_end_when_input_empty() {
                return Some(sample1);
            } else {
                self.current = self.silence();
                self.next_sample = self.current.next();
                return Some(sample1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::buffer::SamplesBuffer;
    use crate::queue;
    use crate::source::Source;

    #[test]
    // #[ignore] // FIXME: samples rate and channel not updated immediately after transition
    fn basic() {
        let (controls, mut source) = queue::queue(false);

        controls.append(SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]));
        controls.append(SamplesBuffer::new(2, 96000, vec![5i16, 5, 5, 5]));

        assert_eq!(source.channels(), 1);
        assert_eq!(source.sample_rate(), 48000);
        assert_eq!(source.next(), Some(10));
        assert_eq!(source.next(), Some(-10));
        assert_eq!(source.next(), Some(10));
        assert_eq!(source.next(), Some(-10));
        assert_eq!(source.channels(), 2);
        assert_eq!(source.sample_rate(), 96000);
        assert_eq!(source.next(), Some(5));
        assert_eq!(source.next(), Some(5));
        assert_eq!(source.next(), Some(5));
        assert_eq!(source.next(), Some(5));
        assert_eq!(source.next(), None);
    }

    #[test]
    fn immediate_end() {
        let (_, mut source) = queue::queue::<i16>(false);
        assert_eq!(source.next(), None);
    }

    #[test]
    fn keep_alive() {
        let (controls, mut source) = queue::queue(true);
        controls.append(SamplesBuffer::new(1, 48000, vec![10i16, -10, 10, -10]));

        assert_eq!(source.next(), Some(10));
        assert_eq!(source.next(), Some(-10));
        assert_eq!(source.next(), Some(10));
        assert_eq!(source.next(), Some(-10));

        for _ in 0..100000 {
            assert_eq!(source.next(), Some(0));
        }
    }

    #[test]
    fn limited_delay_when_added() {
        let (controls, mut source) = queue::queue(true);

        for _ in 0..500 {
            assert_eq!(source.next(), Some(0));
        }

        controls.append(SamplesBuffer::new(4, 41000, vec![10i16, -10, 10, -10]));
        let sample_rate = source.sample_rate() as f64;
        let channels = source.channels() as f64;
        let delay_samples = source.by_ref().take_while(|s| *s == 0).count();
        let delay = Duration::from_secs_f64(delay_samples as f64 / channels / sample_rate);
        assert!(delay < Duration::from_millis(5));

        // assert_eq!(source.next(), Some(10)); // we lose this in the take_while
        assert_eq!(source.next(), Some(-10));
        assert_eq!(source.next(), Some(10));
        assert_eq!(source.next(), Some(-10));
    }
}
