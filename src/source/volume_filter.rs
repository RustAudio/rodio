use std::sync::Arc;
use std::time::Duration;
use std::sync::Mutex;

use Sample;
use Source;

/// Filter that allows another thread to set the volume concurrently.
#[derive(Clone, Debug)]
pub struct VolumeFilter<I>
    where I: Source,
          I::Item: Sample
{
    input: I,

    // Local storage of the volume value.  Allows us to only check the remote occasionally.
    local_volume: f32,

    // The volume value which may be manipulated by another thread.
    remote_volume: Arc<Mutex<f32>>,

    // The frequency with which local_volume should be updated by remote_volume
    update_frequency: u32,

    // How many samples remain until it is time to update local_volume with remote_volume.
    samples_until_update: u32,
}

impl<I> VolumeFilter<I>
    where I: Source,
          I::Item: Sample
{
    pub fn new(source: I, remote_volume: Arc<Mutex<f32>>, update_ms: u32) -> VolumeFilter<I> {
        // TODO: handle the fact that the samples rate can change
        let update_frequency = (update_ms * source.get_samples_rate()) / 1000;
        VolumeFilter {
            input: source,
            local_volume: *remote_volume.lock().unwrap(),
            remote_volume: remote_volume.clone(),
            update_frequency: update_frequency,
            samples_until_update: update_frequency,
        }
    }
}

impl<I> Iterator for VolumeFilter<I>
    where I: Source,
          I::Item: Sample
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        self.samples_until_update -= 1;
        if self.samples_until_update == 0 {
            self.local_volume = *self.remote_volume.lock().unwrap();
            self.samples_until_update = self.update_frequency;
        }
        let next = self.input.next();
        if let Some(sample) = next {
            return Some(sample.amplify(self.local_volume));
        }
        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> Source for VolumeFilter<I>
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
