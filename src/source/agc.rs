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
        attack_time,
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
    attack_time: f32,
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
    /// Sets a new target output level.
    ///
    /// This method allows dynamic adjustment of the target output level
    /// for the Automatic Gain Control. The target level determines the
    /// desired amplitude of the processed audio signal.
    #[inline]
    pub fn set_target_level(&mut self, level: f32) {
        self.target_level = level;
    }

    /// Sets a new absolute maximum gain limit.
    #[inline]
    pub fn set_absolute_max_gain(&mut self, max_gain: f32) {
        self.absolute_max_gain = max_gain;
    }

    /// This method allows changing the attack coefficient dynamically.
    /// The attack coefficient determines how quickly the AGC responds to level changes.
    /// A smaller value results in faster response, while a larger value gives a slower response.
    #[inline]
    pub fn set_attack_coeff(&mut self, attack_time: f32) {
        let sample_rate = self.input.sample_rate();
        self.attack_coeff = (-1.0 / (attack_time * sample_rate as f32)).exp();
    }

    /// Updates the peak level with an adaptive attack coefficient
    ///
    /// This method adjusts the peak level using a variable attack coefficient.
    /// It responds faster to sudden increases in signal level by using a
    /// minimum attack coefficient of 0.1 when the sample value exceeds the
    /// current peak level. This adaptive behavior helps capture transients
    /// more accurately while maintaining smoother behavior for gradual changes.
    #[inline]
    fn update_peak_level(&mut self, sample_value: f32) {
        let attack_coeff = if sample_value > self.peak_level {
            self.attack_coeff.min(0.1) // Faster response to sudden increases
        } else {
            self.attack_coeff
        };
        self.peak_level = attack_coeff * self.peak_level + (1.0 - attack_coeff) * sample_value;
    }

    /// Calculate gain adjustments based on peak and RMS levels
    /// This method determines the appropriate gain level to apply to the audio
    /// signal, considering both peak and RMS (Root Mean Square) levels.
    /// The peak level helps prevent sudden spikes, while the RMS level
    /// provides a measure of the overall signal power over time.
    #[inline]
    fn calculate_peak_gain(&self) -> f32 {
        if self.peak_level > 0.0 {
            self.target_level / self.peak_level
        } else {
            1.0
        }
    }

    /// Updates the RMS (Root Mean Square) level using a sliding window approach.
    /// This method calculates a moving average of the squared input samples,
    /// providing a measure of the signal's average power over time.
    #[inline]
    fn update_rms(&mut self, sample_value: f32) -> f32 {
        // Remove the oldest sample from the RMS calculation
        self.rms_level -= self.rms_window[self.rms_index] / self.rms_window.len() as f32;

        // Add the new sample to the window
        self.rms_window[self.rms_index] = sample_value * sample_value;

        // Add the new sample to the RMS calculation
        self.rms_level += self.rms_window[self.rms_index] / self.rms_window.len() as f32;

        // Move the index to the next position
        self.rms_index = (self.rms_index + 1) % self.rms_window.len();

        // Calculate and return the RMS value
        self.rms_level.sqrt()
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
            // Convert the sample to its absolute float value for level calculations
            let sample_value = value.to_f32().abs();

            // Dynamically adjust peak level using an adaptive attack coefficient
            self.update_peak_level(sample_value);

            // Calculate the current RMS (Root Mean Square) level using a sliding window approach
            let rms = self.update_rms(sample_value);

            // Determine the gain adjustment needed based on the current peak level
            let peak_gain = self.calculate_peak_gain();

            // Compute the gain adjustment required to reach the target level based on RMS
            let rms_gain = if rms > 0.0 {
                self.target_level / rms
            } else {
                1.0 // Default to unity gain if RMS is zero to avoid division by zero
            };

            // Select the lower of peak and RMS gains to ensure conservative adjustment
            let desired_gain = peak_gain.min(rms_gain);

            // Adaptive attack/release speed for AGC (Automatic Gain Control)
            //
            // This mechanism implements an asymmetric approach to gain adjustment:
            // 1. Slow increase: Prevents abrupt amplification of noise during quiet periods.
            // 2. Fast decrease: Rapidly attenuates sudden loud signals to avoid distortion.
            //
            // The asymmetry is crucial because:
            // - Gradual gain increases sound more natural and less noticeable to listeners.
            // - Quick gain reductions are necessary to prevent clipping and maintain audio quality.
            //
            // This approach addresses several challenges associated with high attack times:
            // 1. Slow response: With a high attack time, the AGC responds very slowly to changes in input level.
            //    This means it takes longer for the gain to adjust to new signal levels.
            // 2. Initial gain calculation: When the audio starts or after a period of silence, the initial gain
            //    calculation might result in a very high gain value, especially if the input signal starts quietly.
            // 3. Overshooting: As the gain slowly increases (due to the high attack time), it might overshoot
            //    the desired level, causing the signal to become too loud.
            // 4. Overcorrection: The AGC then tries to correct this by reducing the gain, but due to the slow response,
            //    it might reduce the gain too much, causing the sound to drop to near-zero levels.
            // 5. Slow recovery: Again, due to the high attack time, it takes a while for the gain to increase
            //    back to the appropriate level.
            //
            // By using a faster release time for decreasing gain, we can mitigate these issues and provide
            // more responsive control over sudden level increases while maintaining smooth gain increases.
            let attack_speed = if desired_gain > self.current_gain {
                // Slower attack for increasing gain to avoid sudden amplification
                self.attack_time.min(10.0)
            } else {
                // Faster release for decreasing gain to prevent overamplification
                // Cap release time at 1.0 to ensure responsiveness
                // This prevents issues with very high attack times:
                // - Avoids overcorrection and near-zero sound levels
                // - Ensures AGC can always correct itself in reasonable time
                // - Maintains ability to quickly attenuate sudden loud signals
                (self.attack_time * 0.1).min(1.0) // Capped faster release time
            };

            // Gradually adjust the current gain towards the desired gain for smooth transitions
            self.current_gain =
                self.current_gain * (1.0 - attack_speed) + desired_gain * attack_speed;

            // Ensure the calculated gain stays within the defined operational range
            self.current_gain = self.current_gain.clamp(0.1, self.absolute_max_gain);

            // Output current gain value for monitoring and debugging purposes
            // Must be deleted before merge:
            // Added flag so its usable without the debug temporarily during development
            if std::env::args().any(|arg| arg == "--debug-gain") {
                println!("Current gain: {}", self.current_gain);
            }

            // Apply the computed gain to the input sample and return the result
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
