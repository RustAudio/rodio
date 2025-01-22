//! Queue that plays sounds one after the other.

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
pub fn queue<S>(keep_alive_if_empty: bool) -> (Arc<SourcesQueueInput<S>>, SourcesQueueOutput<S>)
where
    S: Sample + Send + 'static,
{
    let input = Arc::new(SourcesQueueInput {
        next_sounds: Mutex::new(VecDeque::new()),
        keep_alive_if_empty: AtomicBool::new(keep_alive_if_empty),
    });

    let output = SourcesQueueOutput {
        current: Box::new(Empty::<S>::new()) as Box<_>,
        input: input.clone(),
        filling_silence: true,
        curr_span_params: std::cell::Cell::new(SpanParams {
            len: 0,
            channels: 2,
            sample_rate: 44_100,
        }),
    };

    (input, output)
}

type Sound<S> = Box<dyn Source<Item = S> + Send>;
/// The input of the queue.
pub struct SourcesQueueInput<S> {
    next_sounds: Mutex<VecDeque<Sound<S>>>,

    // See constructor.
    keep_alive_if_empty: AtomicBool,
}

impl<S> SourcesQueueInput<S>
where
    S: Sample + Send + 'static,
{
    /// Adds a new source to the end of the queue.
    ///
    /// If silence was playing it can take up to <TODO> milliseconds before
    /// the new sound is played.
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
pub struct SourcesQueueOutput<S> {
    curr_span_params: std::cell::Cell<SpanParams>,

    // The current iterator that produces samples.
    current: Box<dyn Source<Item = S> + Send>,

    // The next sounds.
    input: Arc<SourcesQueueInput<S>>,

    filling_silence: bool,
}

impl<S> Source for SourcesQueueOutput<S>
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

        if self.span_done() {
            return if let Some(next) = self.next_non_zero_span() {
                self.curr_span_params.set(next);
                Some(self.curr_span_params.get().len)
            } else if self.should_end_when_input_empty() {
                Some(0)
            } else {
                Some(self.silence_span_len())
            };
        }

        if self.filling_silence {
            // since this is silence the remaining span len never None
            // and the `if self.span_done()` guarantees its not zero.
            self.current.current_span_len()
        } else if let Some(len) = self.current.current_span_len() {
            Some(len) // Not zero since `self.span_done` is false
        } else if self.current.size_hint().0 > 0 {
            Some(self.current.size_hint().0)
        } else {
            Some(self.fallback_span_length())
        }
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        // could have been called before curr_span_params is update
        // check if they need updating
        if self.span_done() {
            if let Some(next) = self.next_non_zero_span() {
                self.curr_span_params.set(next);
            }
        }
        self.curr_span_params.get().channels
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        // could have been called before curr_span_params is update
        // check if they need updating
        if self.span_done() {
            if let Some(next) = self.next_non_zero_span() {
                self.curr_span_params.set(next);
            }
        }
        self.curr_span_params.get().sample_rate
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

impl<S> Iterator for SourcesQueueOutput<S>
where
    S: Sample + Send + 'static,
{
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<S> {
        // returning None is never acceptable unless the queue should end

        // might need to retry for an unknown amount of times, next (few) queue item
        // could be zero samples long (`EmptyCallback` & friends)
        loop {
            // Basic situation that will happen most of the time.
            if let Some(sample) = self.current.next() {
                return Some(sample);
            }

            if let Some(next) = self.next_sound() {
                self.current = next;
                if let Some(params) = self.next_non_zero_span() {
                    self.curr_span_params.set(params);
                }
                self.filling_silence = false;
            } else if self.should_end_when_input_empty() {
                return None;
            } else {
                self.current = self.silence();
                self.filling_silence = true;
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
    fn fallback_span_length(&self) -> usize {
        200 * self.channels() as usize
    }

    fn silence_span_len(&self) -> usize {
        200 * self.channels() as usize
    }

    fn silence(&self) -> Sound<S> {
        let samples = self.silence_span_len();
        // silence matches span params to make sure resampling
        // gives not popping. It also makes the queue code simpler
        let silence = Zero::<S>::new_samples(
            self.curr_span_params.get().channels,
            self.curr_span_params.get().sample_rate,
            samples,
        );
        Box::new(silence)
    }

    fn should_end_when_input_empty(&self) -> bool {
        !self.input.keep_alive_if_empty.load(Ordering::Acquire)
    }

    fn next_non_zero_span(&self) -> Option<SpanParams> {
        dbg!(self.input
            .next_sounds
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.current_span_len().is_some_and(|len| len > 0))
            .map(|s| SpanParams {
                len: s.current_span_len().expect("filter checks this"),
                channels: s.channels(),
                sample_rate: s.sample_rate(),
            })
            .next())
    }

    fn next_sound(&self) -> Option<Sound<S>> {
        self.input.next_sounds.lock().unwrap().pop_front()
    }

    fn span_done(&self) -> bool {
        if let Some(left) = self.current.current_span_len() {
            left == 0
        } else {
            self.current.size_hint().0 == 0
        }
    }
}

#[derive(Debug, Copy, Clone)]
struct SpanParams {
    len: usize,
    channels: ChannelCount,
    sample_rate: SampleRate,
}

#[cfg(test)]
mod tests {
    use crate::buffer::SamplesBuffer;
    use crate::queue;
    use crate::source::Source;

    #[test]
    // #[ignore] // FIXME: samples rate and channel not updated immediately after transition
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
