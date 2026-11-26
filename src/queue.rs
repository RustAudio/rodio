//! Queue that plays sounds one after the other.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::source::{Empty, SeekError, Source};
use crate::Sample;

use crate::common::{ChannelCount, SampleRate};
#[cfg(feature = "crossbeam-channel")]
use crossbeam_channel::{unbounded as channel, Receiver, Sender};
#[cfg(not(feature = "crossbeam-channel"))]
use std::sync::mpsc::{channel, Receiver, Sender};

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
pub fn queue(keep_alive_if_empty: bool) -> (Arc<SourcesQueueInput>, SourcesQueueOutput) {
    let input = Arc::new(SourcesQueueInput {
        next_sounds: Mutex::new(VecDeque::new()),
        keep_alive_if_empty: AtomicBool::new(keep_alive_if_empty),
    });

    let output = SourcesQueueOutput {
        current: Box::new(Empty::new()) as Box<_>,
        signal_after_end: None,
        input: input.clone(),
        samples_in_current_frame: 0,
        silence_samples_remaining: 0,
        current_ended: false,
        prefetched_frame: VecDeque::new(),
        prefetched_format: None,
    };

    (input, output)
}

// TODO: consider reimplementing this with `from_factory`

type Sound = Box<dyn Source + Send>;
type SignalDone = Option<Sender<()>>;

/// The input of the queue.
pub struct SourcesQueueInput {
    next_sounds: Mutex<VecDeque<(Sound, SignalDone)>>,

    // See constructor.
    keep_alive_if_empty: AtomicBool,
}

impl SourcesQueueInput {
    /// Adds a new source to the end of the queue.
    #[inline]
    pub fn append<T>(&self, source: T)
    where
        T: Source + Send + 'static,
    {
        self.next_sounds
            .lock()
            .unwrap()
            .push_back((Box::new(source) as Box<_>, None));
    }

    /// Adds a new source to the end of the queue.
    ///
    /// The `Receiver` will be signalled when the sound has finished playing.
    ///
    /// Enable the feature flag `crossbeam-channel` in rodio to use a `crossbeam_channel::Receiver`
    /// instead.
    #[inline]
    pub fn append_with_signal<T>(&self, source: T) -> Receiver<()>
    where
        T: Source + Send + 'static,
    {
        let (tx, rx) = channel();
        self.next_sounds
            .lock()
            .unwrap()
            .push_back((Box::new(source) as Box<_>, Some(tx)));
        rx
    }

    /// Sets whether the queue stays alive if there's no more sound to play.
    ///
    /// See also the constructor.
    pub fn set_keep_alive_if_empty(&self, keep_alive_if_empty: bool) {
        self.keep_alive_if_empty
            .store(keep_alive_if_empty, Ordering::Release);
    }

    /// Returns whether the queue stays alive if there's no more sound to play.
    pub fn keep_alive_if_empty(&self) -> bool {
        self.keep_alive_if_empty.load(Ordering::Acquire)
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
pub struct SourcesQueueOutput {
    // The current iterator that produces samples.
    current: Box<dyn Source + Send>,

    // Signal this sender before picking from `next`.
    signal_after_end: Option<Sender<()>>,

    // The next sounds.
    input: Arc<SourcesQueueInput>,

    samples_in_current_frame: usize,

    // When a source ends mid-frame, this counts how many silence samples to inject
    // to complete the frame before transitioning to the next source.
    silence_samples_remaining: usize,

    current_ended: bool,
    prefetched_frame: VecDeque<Sample>,
    prefetched_format: Option<(ChannelCount, SampleRate)>,
}

impl Source for SourcesQueueOutput {
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        if let Some((channels, _)) = self.prefetched_format {
            return Some(usize::from(channels.get()));
        }
        // A queue can change formats even when the current source has an unknown span length.
        Some(self.with_source(|source| {
            source
                .current_span_len()
                .filter(|&len| len != 0)
                .unwrap_or_else(|| usize::from(source.channels().get()))
        }))
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        if let Some((channels, _)) = self.prefetched_format {
            channels
        } else {
            self.with_source(|source| source.channels())
        }
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        if let Some((_, sample_rate)) = self.prefetched_format {
            sample_rate
        } else {
            self.with_source(|source| source.sample_rate())
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
    // That no longer seems in line with the queue behaviour.
    //
    // A final pain point is that we would need the total duration for the
    // next few songs.
    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.current.try_seek(pos)?;
        self.current_ended = false;
        self.samples_in_current_frame %= usize::from(self.current.channels().get());
        if self.prefetched_format.take().is_some() {
            self.prefetched_frame.clear();
            // The prefetched frame left the input at a frame boundary. Restore the
            // channel position already delivered to the output in the current frame.
            for _ in 0..self.samples_in_current_frame {
                if self.current.next().is_none() {
                    self.current_ended = true;
                    break;
                }
            }
        }
        Ok(())
    }
}

impl Iterator for SourcesQueueOutput {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If we're padding to complete a frame, return silence.
            if self.silence_samples_remaining > 0 {
                self.silence_samples_remaining -= 1;
                return Some(0.0);
            }

            let channels = self
                .prefetched_format
                .map_or_else(|| self.current.channels(), |(channels, _)| channels);
            let channels = usize::from(channels.get());
            let sample = if let Some(sample) = self.prefetched_frame.pop_front() {
                if self.prefetched_frame.is_empty() {
                    self.prefetched_format = None;
                }
                Some(sample)
            } else if self.current_ended {
                None
            } else {
                self.current.next()
            };
            if let Some(sample) = sample {
                self.samples_in_current_frame = (self.samples_in_current_frame + 1) % channels;
                if self.samples_in_current_frame == 0
                    && !self.current_ended
                    && self.current.current_span_len().is_none()
                {
                    // Let the next metadata query see EOF before the next source is read.
                    self.prefetch_frame();
                }
                return Some(sample);
            }

            // Source ended - check if we ended mid-frame and need padding.
            self.current_ended = true;
            let incomplete_frame_samples = self.samples_in_current_frame % channels;
            if incomplete_frame_samples > 0 {
                // We're mid-frame - need to pad with silence to complete it.
                self.silence_samples_remaining = channels - incomplete_frame_samples;
                self.samples_in_current_frame = 0;
                // Continue loop - next iteration will inject silence.
                continue;
            }

            // In order to avoid inlining this expensive operation, the code is in another function.
            if self.go_next().is_err() {
                if self.input.keep_alive_if_empty() {
                    self.silence_samples_remaining = self.current.channels().get() as usize;
                    continue;
                } else {
                    return None;
                }
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let lower = if self.current_ended {
            0
        } else {
            self.current.size_hint().0
        };
        (lower.saturating_add(self.prefetched_frame.len()), None)
    }
}

impl SourcesQueueOutput {
    fn prefetch_frame(&mut self) {
        let channels = self.current.channels();
        let sample_rate = self.current.sample_rate();
        let frame_len = usize::from(channels.get());
        self.prefetched_frame
            .extend(self.current.by_ref().take(frame_len));
        self.current_ended = self.prefetched_frame.len() < frame_len;
        if !self.prefetched_frame.is_empty() {
            self.prefetched_format = Some((channels, sample_rate));
        }
    }

    // Peek at the next sound's format after EOF without removing it from the queue.
    fn with_source<T>(&self, f: impl FnOnce(&dyn Source) -> T) -> T {
        if (self.current_ended || self.current.is_exhausted())
            && self.samples_in_current_frame == 0
            && self.silence_samples_remaining == 0
        {
            let sounds = self.input.next_sounds.lock().unwrap();
            if let Some((next, _)) = sounds.iter().find(|(source, _)| !source.is_exhausted()) {
                return f(next.as_ref());
            }
        }
        f(self.current.as_ref())
    }

    // Called when `current` is empty, and we must jump to the next element.
    // Returns `Ok` if there is another sound should continue playing, or `Err` when there is not.
    //
    // This method is separate so that it is not inlined.
    fn go_next(&mut self) -> Result<(), ()> {
        if let Some(signal_after_end) = self.signal_after_end.take() {
            let _ = signal_after_end.send(());
        }

        let (next, signal_after_end) = {
            let mut next = self.input.next_sounds.lock().unwrap();
            next.pop_front().ok_or(())?
        };

        self.current = next;
        self.signal_after_end = signal_after_end;
        self.current_ended = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::buffer::SamplesBuffer;
    use crate::math::nz;
    use crate::source::{chain, from_iter, SeekError, Source, UniformSourceIterator};
    use crate::{queue, ChannelCount, Sample, SampleRate};
    use std::time::Duration;

    #[test]
    fn queue_keeps_format_boundaries_when_current_source_has_no_span_length() {
        let (tx, mut rx) = queue::queue(false);
        let mut samples = [0.125, 0.25, 0.375].into_iter();
        tx.append(from_iter(
            std::iter::from_fn(move || samples.next()),
            nz!(1),
            nz!(1),
        ));
        tx.append(SamplesBuffer::new(nz!(2), nz!(1), [0.5, -0.5, 0.75, -0.75]));
        tx.append(SamplesBuffer::new(nz!(1), nz!(1), [0.875, 1.0]));
        assert_eq!(rx.next(), Some(0.125));

        let source = UniformSourceIterator::new(rx, nz!(1), nz!(1));
        assert_eq!(
            source.collect::<Vec<_>>(),
            [0.25, 0.375, 0.5, 0.75, 0.875, 1.0]
        );
    }

    #[test]
    fn queue_seek_preserves_channel_order_with_a_prefetched_frame() {
        for channel in 0..2 {
            let (tx, mut rx) = queue::queue(false);
            let samples =
                SamplesBuffer::new(nz!(2), nz!(1), [0.125, -0.125, 0.25, -0.25, 0.5, -0.5]);
            tx.append(UniformSourceIterator::new(samples, nz!(2), nz!(1)));
            assert_eq!(rx.by_ref().take(2 + channel).count(), 2 + channel);

            rx.try_seek(Duration::from_secs(2)).unwrap();
            assert_eq!(rx.collect::<Vec<_>>(), [0.5, -0.5][channel..]);
        }
    }

    #[test]
    #[ignore = "known limitation: metadata gap when queue is briefly empty after exhaustion"]
    fn metadata_gap_when_queue_briefly_empty() {
        let new_rate = nz!(48000);
        let (tx, mut rx) = queue::queue(false);
        tx.append(SamplesBuffer::new(nz!(1), nz!(44100), vec![1.0]));
        assert_eq!(rx.next(), Some(1.0));

        // Source is exhausted, nothing queued yet. A real consumer reads metadata here
        // to set up its converter — it gets the stale value.
        let rate_seen_by_consumer = rx.sample_rate();

        // The replacement source arrives only after the metadata was already queried.
        tx.append(SamplesBuffer::new(nz!(1), new_rate, vec![2.0]));

        // Ideally the consumer would have seen 48000. In practice it saw 44100.
        assert_eq!(rate_seen_by_consumer, new_rate);
    }

    #[test]
    fn exhausted_source_in_queue_is_skipped_for_metadata() {
        let source_rate = nz!(48000);
        // The empty chain's dummy rate must differ from source_rate, otherwise the test
        // would not catch the bug (both values would satisfy the assertion below).
        let empty_chain_dummy_rate = chain(std::iter::empty::<SamplesBuffer>()).sample_rate();
        assert_ne!(empty_chain_dummy_rate, source_rate);

        let (tx, mut rx) = queue::queue(false);
        tx.append(chain(std::iter::empty::<SamplesBuffer>()));
        tx.append(SamplesBuffer::new(nz!(1), source_rate, vec![1.0, 2.0]));

        assert_eq!(rx.channels(), nz!(1));
        assert_eq!(rx.sample_rate(), source_rate);
        assert_eq!(rx.next(), Some(1.0));
        assert_eq!(rx.next(), Some(2.0));
        assert_eq!(rx.next(), None);
    }

    #[test]
    fn basic() {
        let (tx, mut rx) = queue::queue(false);

        tx.append(SamplesBuffer::new(
            nz!(1),
            nz!(48000),
            vec![10.0, -10.0, 10.0, -10.0],
        ));
        tx.append(SamplesBuffer::new(
            nz!(2),
            nz!(96000),
            vec![5.0, 5.0, 5.0, 5.0],
        ));

        assert_eq!(rx.channels(), nz!(1));
        assert_eq!(rx.sample_rate().get(), 48000);
        assert_eq!(rx.next(), Some(10.0));
        assert_eq!(rx.next(), Some(-10.0));
        assert_eq!(rx.next(), Some(10.0));
        assert_eq!(rx.next(), Some(-10.0));
        assert_eq!(rx.channels(), nz!(2));
        assert_eq!(rx.sample_rate().get(), 96000);
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), Some(5.0));
        assert_eq!(rx.next(), None);
    }

    #[test]
    fn immediate_end() {
        let (_, mut rx) = queue::queue(false);
        assert_eq!(rx.next(), None);
    }

    #[test]
    fn keep_alive() {
        let (tx, mut rx) = queue::queue(true);
        tx.append(SamplesBuffer::new(
            nz!(1),
            nz!(48000),
            vec![10.0, -10.0, 10.0, -10.0],
        ));

        assert_eq!(rx.next(), Some(10.0));
        assert_eq!(rx.next(), Some(-10.0));
        assert_eq!(rx.next(), Some(10.0));
        assert_eq!(rx.next(), Some(-10.0));

        for _ in 0..100000 {
            assert_eq!(rx.next(), Some(0.0));
        }
    }

    #[test]
    fn no_delay_when_added() {
        let (tx, mut rx) = queue::queue(true);

        for _ in 0..500 {
            assert_eq!(rx.next(), Some(0.0));
        }

        tx.append(SamplesBuffer::new(
            nz!(1),
            nz!(48000),
            vec![10.0, -10.0, 10.0, -10.0],
        ));
        assert_eq!(rx.next(), Some(10.0));
        assert_eq!(rx.next(), Some(-10.0));
        assert_eq!(rx.next(), Some(10.0));
        assert_eq!(rx.next(), Some(-10.0));
    }

    #[test]
    fn sample_rate_correct_after_stopped_source() {
        let (tx, mut rx) = queue::queue(true);

        let mut stopped_source = SamplesBuffer::new(nz!(1), nz!(48000), vec![0.0; 100]).stoppable();
        stopped_source.stop();

        let new_source = SamplesBuffer::new(nz!(1), nz!(22050), vec![0.5; 100]);
        assert_ne!(stopped_source.sample_rate(), new_source.sample_rate());
        let new_sample_rate = new_source.sample_rate();

        // Pull one sample so keep-alive behavior is triggered.
        tx.append(stopped_source);
        let _ = rx.next();

        tx.append(new_source);
        assert_eq!(rx.sample_rate(), new_sample_rate);
    }

    #[test]
    fn sample_rate_correct_after_skipped_source() {
        let (tx, mut rx) = queue::queue(true);

        let mut skipped_source = SamplesBuffer::new(nz!(1), nz!(48000), vec![0.0; 100]).skippable();
        crate::source::Skippable::skip(&mut skipped_source);

        let new_source = SamplesBuffer::new(nz!(1), nz!(22050), vec![0.5; 100]);
        assert_ne!(skipped_source.sample_rate(), new_source.sample_rate());
        let new_sample_rate = new_source.sample_rate();

        // Pull one sample so keep-alive behavior is triggered.
        tx.append(skipped_source);
        let _ = rx.next();

        tx.append(new_source);
        assert_eq!(rx.sample_rate(), new_sample_rate);
    }

    #[test]
    fn channel_correct_on_first_append() {
        let (mixer_tx, mut mixer_rx) = crate::mixer::mixer(nz!(2), nz!(48000));
        let (tx, rx) = queue::queue(true);

        assert_eq!(rx.channels(), nz!(1), "initial channels should be 1");
        mixer_tx.add(rx);

        tx.append(SamplesBuffer::new(
            nz!(2),
            nz!(48000),
            vec![1.0, -1.0, 1.0, -1.0],
        ));

        assert_eq!(mixer_rx.next(), Some(1.0), "expected L");
        assert_eq!(mixer_rx.next(), Some(-1.0), "expected R");
        assert_eq!(mixer_rx.next(), Some(1.0), "expected L");
        assert_eq!(mixer_rx.next(), Some(-1.0), "expected R");
    }

    #[test]
    fn append_updates_metadata() {
        for keep_alive in [false, true] {
            let (tx, rx) = queue::queue(keep_alive);
            assert_eq!(
                rx.channels(),
                nz!(1),
                "Initial channels should be 1 (keep_alive={keep_alive})"
            );
            assert_eq!(
                rx.sample_rate(),
                crate::DEFAULT_SAMPLE_RATE,
                "Initial sample rate should be DEFAULT_SAMPLE_RATE (keep_alive={keep_alive})"
            );

            tx.append(SamplesBuffer::new(
                nz!(2),
                nz!(44100),
                vec![0.1, 0.2, 0.3, 0.4],
            ));

            assert_eq!(
                rx.channels(),
                nz!(2),
                "Channels should update to 2 (keep_alive={keep_alive})"
            );
            assert_eq!(
                rx.sample_rate(),
                nz!(44100),
                "Sample rate should update to 44100 (keep_alive={keep_alive})"
            );
        }
    }
}
