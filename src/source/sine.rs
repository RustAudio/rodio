use std::time::Duration;
use Source;

const PI_2: f32 = std::f32::consts::PI * 2.0f32;

/// An infinite source that produces a sine.
///
/// Always has a rate of 48kHz and one channel.
#[derive(Clone, Debug)]
pub struct SineWave {
    freq: f32,
    cur_val: f32,
}

impl SineWave {
    /// The frequency of the sine.
    #[inline]
    pub fn new(freq: u32) -> SineWave {
        SineWave {
            freq: freq as f32,
            cur_val: 0.0,
        }
    }
}

impl Iterator for SineWave {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        self.cur_val += PI_2 * self.freq / 48000.0;
        if self.cur_val > PI_2  {
            self.cur_val -= PI_2;
        }
        Some(self.cur_val.sin())
    }
}

impl Source for SineWave {
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> u16 {
        1
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        48000
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
