use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use Sample;
use Source;

/// Filter that allows another thread to pause the stream.
#[derive(Clone, Debug)]
pub struct Pauseable<I>
    where I: Source,
          I::Item: Sample
{
    input: I,

    // Local storage of the paused value.  Allows us to only check the remote occasionally.
    local_paused: bool,

    // The paused value which may be manipulated by another thread.
    remote_paused: Arc<AtomicBool>,

    // The frequency with which local_paused should be updated by remote_paused
    update_frequency: u32,

    // How many samples remain until it is time to update local_paused with remote_paused.
    samples_until_update: u32,
}

impl<I> Pauseable<I>
    where I: Source,
          I::Item: Sample
{
    pub fn new(source: I, remote_paused: Arc<AtomicBool>, update_ms: u32) -> Pauseable<I> {
        // TODO: handle the fact that the samples rate can change
        let update_frequency = (update_ms * source.get_samples_rate()) / 1000;
        Pauseable {
            input: source,
            local_paused: remote_paused.load(Ordering::Relaxed),
            remote_paused: remote_paused,
            update_frequency: update_frequency,
            samples_until_update: update_frequency,
        }
    }
}

impl<I> Iterator for Pauseable<I>
    where I: Source,
          I::Item: Sample
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        self.samples_until_update -= 1;
        if self.samples_until_update == 0 {
            self.local_paused = self.remote_paused.load(Ordering::Relaxed);
            self.samples_until_update = self.update_frequency;
        }
        if self.local_paused {
            return Some(I::Item::zero_value());
        }
        self.input.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> Source for Pauseable<I>
    where I: Source,
          I::Item: Sample
{
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        self.input.get_current_frame_len()
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        self.input.get_channels()
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        self.input.get_samples_rate()
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        self.input.get_total_duration()
    }
}
