use crate::source::ChannelVolume;
use crate::Sample;
use crate::Source;
use std::fmt::Debug;
use std::time::Duration;

/// Combines channels in input into a single mono source, then plays that mono sound
/// to each channel at the volume given for that channel.
#[derive(Clone, Debug)]
pub struct Spatial<I>
where
    I: Source,
    I::Item: Sample + Debug,
{
    input: ChannelVolume<I>,
}

fn dist_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<f32>()
}

impl<I> Spatial<I>
where
    I: Source,
    I::Item: Sample + Debug,
{
    pub fn new(
        input: I,
        emitter_position: [f32; 3],
        left_ear: [f32; 3],
        right_ear: [f32; 3],
    ) -> Spatial<I>
    where
        I: Source,
        I::Item: Sample,
    {
        let mut ret = Spatial {
            input: ChannelVolume::new(input, vec![0.0, 0.0]),
        };
        ret.set_positions(emitter_position, left_ear, right_ear);
        ret
    }

    /// Sets the position of the emitter and ears in the 3D world.
    pub fn set_positions(
        &mut self,
        emitter_pos: [f32; 3],
        left_ear: [f32; 3],
        right_ear: [f32; 3],
    ) {
        let left_dist_sq = dist_sq(left_ear, emitter_pos);
        let right_dist_sq = dist_sq(right_ear, emitter_pos);
        let max_diff = dist_sq(left_ear, right_ear).sqrt();
        let left_dist = left_dist_sq.sqrt();
        let right_dist = right_dist_sq.sqrt();
        let left_diff_modifier = ((left_dist - right_dist) / max_diff + 1.0) / 4.0 + 0.5;
        let right_diff_modifier = ((right_dist - left_dist) / max_diff + 1.0) / 4.0 + 0.5;
        let left_dist_modifier = (1.0 / left_dist_sq).min(1.0);
        let right_dist_modifier = (1.0 / right_dist_sq).min(1.0);
        self.input
            .set_volume(0, left_diff_modifier * left_dist_modifier);
        self.input
            .set_volume(1, right_diff_modifier * right_dist_modifier);
    }
}

impl<I> Iterator for Spatial<I>
where
    I: Source,
    I::Item: Sample + Debug,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        self.input.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for Spatial<I>
where
    I: Source + ExactSizeIterator,
    I::Item: Sample + Debug,
{
}

impl<I> Source for Spatial<I>
where
    I: Source,
    I::Item: Sample + Debug,
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
    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
}
