use super::SeekError;
use crate::{Sample, Source};
use std::time::Duration;

/// Constructs an `AutomaticGainControl` object with specified parameters.
///
/// # Arguments
///
/// * `input` - The input audio source
/// * `target_level` - The desired output level
/// * `attack_time` - Time constant for gain adjustment
/// * `absolute_max_gain` - Maximum allowable gain
pub fn automatic_gain_control<I>(
    input: I,
    target_level: f32,
    attack_time: f32,
    absolute_max_gain: f32,
) -> AutomaticGainControl<I>
where
    I: Source,
    I::Item: Sample,
{
    let sample_rate = input.sample_rate();

    AutomaticGainControl {
        input,
        target_level,
        absolute_max_gain,
        current_gain: 1.0,
        attack_coeff: (-1.0 / (attack_time * sample_rate as f32)).exp(),
        peak_level: 0.0,
        rms_level: 0.0,
        rms_window: vec![0.0; 1024],
        rms_index: 0,
    }
}

/// Automatic Gain Control filter for maintaining consistent output levels.
#[derive(Clone, Debug)]
pub struct AutomaticGainControl<I> {
    input: I,
    target_level: f32,
    absolute_max_gain: f32,
    current_gain: f32,
    attack_coeff: f32,
    peak_level: f32,
    rms_level: f32,
    rms_window: Vec<f32>,
    rms_index: usize,
}

impl<I> AutomaticGainControl<I>
where
    I: Source,
    I::Item: Sample,
{
    // Sets a new target output level.
    #[inline]
    pub fn set_target_level(&mut self, level: f32) {
        self.target_level = level;
    }

    // Add this method to allow changing the attack coefficient
    pub fn set_attack_coeff(&mut self, attack_time: f32) {
        let sample_rate = self.input.sample_rate();
        self.attack_coeff = (-1.0 / (attack_time * sample_rate as f32)).exp();
    }
}

impl<I> Iterator for AutomaticGainControl<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        self.input.next().map(|value| {
            let sample_value = value.to_f32().abs();

            // Update peak level with adaptive attack coefficient
            let attack_coeff = if sample_value > self.peak_level {
                self.attack_coeff.min(0.1) // Faster response to sudden increases
            } else {
                self.attack_coeff
            };
            self.peak_level = attack_coeff * self.peak_level + (1.0 - attack_coeff) * sample_value;

            // Update RMS level using a sliding window
            self.rms_level -= self.rms_window[self.rms_index] / self.rms_window.len() as f32;
            self.rms_window[self.rms_index] = sample_value * sample_value;
            self.rms_level += self.rms_window[self.rms_index] / self.rms_window.len() as f32;
            self.rms_index = (self.rms_index + 1) % self.rms_window.len();

            let rms = self.rms_level.sqrt();

            // Calculate gain adjustments based on peak and RMS levels
            let peak_gain = if self.peak_level > 0.0 {
                self.target_level / self.peak_level
            } else {
                1.0
            };

            let rms_gain = if rms > 0.0 {
                self.target_level / rms
            } else {
                1.0
            };

            // Choose the more conservative gain adjustment
            let desired_gain = peak_gain.min(rms_gain);

            // Set target gain to the middle of the allowable range
            let target_gain = 1.0; // Midpoint between 0.1 and 3.0

            // Smoothly adjust current gain towards the target
            let adjustment_speed = 0.05; // Balance between responsiveness and stability
            self.current_gain = self.current_gain * (1.0 - adjustment_speed)
                + (desired_gain * target_gain) * adjustment_speed;

            // Constrain gain within predefined limits
            self.current_gain = self.current_gain.clamp(0.1, self.absolute_max_gain);

            // Uncomment for debugging:
            println!("Current gain: {}", self.current_gain);

            // Apply calculated gain to the sample
            value.amplify(self.current_gain)
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for AutomaticGainControl<I>
where
    I: Source + ExactSizeIterator,
    I::Item: Sample,
{
}

impl<I> Source for AutomaticGainControl<I>
where
    I: Source,
    I::Item: Sample,
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

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.input.try_seek(pos)
    }
}
