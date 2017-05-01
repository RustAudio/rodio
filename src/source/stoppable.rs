use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use Sample;
use Source;

/// Filter that allows another thread to pause the stream.
#[derive(Clone, Debug)]
pub struct Stoppable<I>
    where I: Source,
          I::Item: Sample
{
    input: I,

    // The paused value which may be manipulated by another thread.
    remote_stopped: Arc<AtomicBool>,

    // The frequency with which remote_stopped is checked.
    update_frequency: u32,

    // How many samples remain until it is time to check remote_stopped.
    samples_until_update: u32,
}

impl<I> Stoppable<I>
    where I: Source,
          I::Item: Sample
{
    pub fn new(source: I, remote_stopped: Arc<AtomicBool>, update_ms: u32) -> Stoppable<I> {
        // TODO: handle the fact that the samples rate can change
        let update_frequency = (update_ms * source.samples_rate()) / 1000;
        Stoppable {
            input: source,
            remote_stopped: remote_stopped,
            update_frequency: update_frequency,
            samples_until_update: update_frequency,
        }
    }
}

impl<I> Iterator for Stoppable<I>
    where I: Source,
          I::Item: Sample
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        if self.samples_until_update == 0 {
            if self.remote_stopped.load(Ordering::Relaxed) {
                return None;
            } else {
                self.samples_until_update = self.update_frequency;
            }
        } else {
            self.samples_until_update -= 1;
        }

        self.input.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> Source for Stoppable<I>
    where I: Source,
          I::Item: Sample
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        self.input.current_frame_len()
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.input.channels()
    }

    #[inline]
    fn samples_rate(&self) -> u32 {
        self.input.samples_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
}
