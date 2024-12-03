//
//      Automatic Gain Control (AGC) Algorithm
//      Designed by @UnknownSuperficialNight
//
//   Features:
//   • Adaptive peak detection
//   • RMS-based level estimation
//   • Asymmetric attack/release
//   • RMS-based general adjustments with peak limiting
//
//   Optimized for smooth and responsive gain control
//
//   Crafted with love. Enjoy! :)
//

use super::SeekError;
use crate::{Sample, Source};
#[cfg(feature = "experimental")]
use atomic_float::AtomicF32;
#[cfg(feature = "experimental")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "experimental")]
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "tracing")]
use tracing;

/// Ensures `RMS_WINDOW_SIZE` is a power of two
const fn power_of_two(n: usize) -> usize {
    assert!(
        n.is_power_of_two(),
        "RMS_WINDOW_SIZE must be a power of two"
    );
    n
}

/// Size of the circular buffer used for RMS calculation.
/// A larger size provides more stable RMS values but increases latency.
const RMS_WINDOW_SIZE: usize = power_of_two(8192);

#[cfg(feature = "experimental")]
/// Automatic Gain Control filter for maintaining consistent output levels.
///
/// This struct implements an AGC algorithm that dynamically adjusts audio levels
/// based on both **peak** and **RMS** (Root Mean Square) measurements.
#[derive(Clone, Debug)]
pub struct AutomaticGainControl<I> {
    input: I,
    target_level: Arc<AtomicF32>,
    absolute_max_gain: Arc<AtomicF32>,
    current_gain: f32,
    attack_coeff: Arc<AtomicF32>,
    release_coeff: Arc<AtomicF32>,
    min_attack_coeff: f32,
    peak_level: f32,
    rms_window: CircularBuffer,
    is_enabled: Arc<AtomicBool>,
}

#[cfg(not(feature = "experimental"))]
/// Automatic Gain Control filter for maintaining consistent output levels.
///
/// This struct implements an AGC algorithm that dynamically adjusts audio levels
/// based on both **peak** and **RMS** (Root Mean Square) measurements.
#[derive(Clone, Debug)]
pub struct AutomaticGainControl<I> {
    input: I,
    target_level: f32,
    absolute_max_gain: f32,
    current_gain: f32,
    attack_coeff: f32,
    release_coeff: f32,
    min_attack_coeff: f32,
    peak_level: f32,
    rms_window: CircularBuffer,
    is_enabled: bool,
}

/// A circular buffer for efficient RMS calculation over a sliding window.
///
/// This structure allows for constant-time updates and mean calculations,
/// which is crucial for real-time audio processing.
#[derive(Clone, Debug)]
struct CircularBuffer {
    buffer: Box<[f32; RMS_WINDOW_SIZE]>,
    sum: f32,
    index: usize,
}

impl CircularBuffer {
    /// Creates a new `CircularBuffer` with a fixed size determined at compile time.
    #[inline]
    fn new() -> Self {
        CircularBuffer {
            buffer: Box::new([0.0; RMS_WINDOW_SIZE]),
            sum: 0.0,
            index: 0,
        }
    }

    /// Pushes a new value into the buffer and returns the old value.
    ///
    /// This method maintains a running sum for efficient mean calculation.
    #[inline]
    fn push(&mut self, value: f32) -> f32 {
        let old_value = self.buffer[self.index];
        // Update the sum by first subtracting the old value and then adding the new value; this is more accurate.
        self.sum = self.sum - old_value + value;
        self.buffer[self.index] = value;
        // Use bitwise AND for efficient index wrapping since RMS_WINDOW_SIZE is a power of two.
        self.index = (self.index + 1) & (RMS_WINDOW_SIZE - 1);
        old_value
    }

    /// Calculates the mean of all values in the buffer.
    ///
    /// This operation is `O(1)` due to the maintained running sum.
    #[inline]
    fn mean(&self) -> f32 {
        self.sum / RMS_WINDOW_SIZE as f32
    }
}

/// Constructs an `AutomaticGainControl` object with specified parameters.
///
/// # Arguments
///
/// * `input` - The input audio source
/// * `target_level` - The desired output level
/// * `attack_time` - Time constant for gain increase
/// * `release_time` - Time constant for gain decrease
/// * `absolute_max_gain` - Maximum allowable gain
#[inline]
pub(crate) fn automatic_gain_control<I>(
    input: I,
    target_level: f32,
    attack_time: f32,
    release_time: f32,
    absolute_max_gain: f32,
) -> AutomaticGainControl<I>
where
    I: Source,
    I::Item: Sample,
{
    let sample_rate = input.sample_rate();
    let attack_coeff = (-1.0 / (attack_time * sample_rate as f32)).exp();
    let release_coeff = (-1.0 / (release_time * sample_rate as f32)).exp();

    #[cfg(feature = "experimental")]
    {
        AutomaticGainControl {
            input,
            target_level: Arc::new(AtomicF32::new(target_level)),
            absolute_max_gain: Arc::new(AtomicF32::new(absolute_max_gain)),
            current_gain: 1.0,
            attack_coeff: Arc::new(AtomicF32::new(attack_coeff)),
            release_coeff: Arc::new(AtomicF32::new(release_coeff)),
            min_attack_coeff: release_time,
            peak_level: 0.0,
            rms_window: CircularBuffer::new(),
            is_enabled: Arc::new(AtomicBool::new(true)),
        }
    }

    #[cfg(not(feature = "experimental"))]
    {
        AutomaticGainControl {
            input,
            target_level,
            absolute_max_gain,
            current_gain: 1.0,
            attack_coeff,
            release_coeff,
            min_attack_coeff: release_time,
            peak_level: 0.0,
            rms_window: CircularBuffer::new(),
            is_enabled: true,
        }
    }
}

impl<I> AutomaticGainControl<I>
where
    I: Source,
    I::Item: Sample,
{
    #[inline]
    fn target_level(&self) -> f32 {
        #[cfg(feature = "experimental")]
        {
            self.target_level.load(Ordering::Relaxed)
        }
        #[cfg(not(feature = "experimental"))]
        {
            self.target_level
        }
    }

    #[inline]
    fn absolute_max_gain(&self) -> f32 {
        #[cfg(feature = "experimental")]
        {
            self.absolute_max_gain.load(Ordering::Relaxed)
        }
        #[cfg(not(feature = "experimental"))]
        {
            self.absolute_max_gain
        }
    }

    #[inline]
    fn attack_coeff(&self) -> f32 {
        #[cfg(feature = "experimental")]
        {
            self.attack_coeff.load(Ordering::Relaxed)
        }
        #[cfg(not(feature = "experimental"))]
        {
            self.attack_coeff
        }
    }

    #[inline]
    fn release_coeff(&self) -> f32 {
        #[cfg(feature = "experimental")]
        {
            self.release_coeff.load(Ordering::Relaxed)
        }
        #[cfg(not(feature = "experimental"))]
        {
            self.release_coeff
        }
    }

    #[inline]
    fn is_enabled(&self) -> bool {
        #[cfg(feature = "experimental")]
        {
            self.is_enabled.load(Ordering::Relaxed)
        }
        #[cfg(not(feature = "experimental"))]
        {
            self.is_enabled
        }
    }

    #[cfg(feature = "experimental")]
    /// Access the target output level for real-time adjustment.
    ///
    /// Use this to dynamically modify the AGC's target level while audio is processing.
    /// Adjust this value to control the overall output amplitude of the processed signal.
    #[inline]
    pub fn get_target_level(&self) -> Arc<AtomicF32> {
        Arc::clone(&self.target_level)
    }

    #[cfg(feature = "experimental")]
    /// Access the maximum gain limit for real-time adjustment.
    ///
    /// Use this to dynamically modify the AGC's maximum allowable gain during runtime.
    /// Adjusting this value helps prevent excessive amplification in low-level signals.
    #[inline]
    pub fn get_absolute_max_gain(&self) -> Arc<AtomicF32> {
        Arc::clone(&self.absolute_max_gain)
    }

    #[cfg(feature = "experimental")]
    /// Access the attack coefficient for real-time adjustment.
    ///
    /// Use this to dynamically modify how quickly the AGC responds to level increases.
    /// Smaller values result in faster response, larger values in slower response.
    /// Adjust during runtime to fine-tune AGC behavior for different audio content.
    #[inline]
    pub fn get_attack_coeff(&self) -> Arc<AtomicF32> {
        Arc::clone(&self.attack_coeff)
    }

    #[cfg(feature = "experimental")]
    /// Access the release coefficient for real-time adjustment.
    ///
    /// Use this to dynamically modify how quickly the AGC responds to level decreases.
    /// Smaller values result in faster response, larger values in slower response.
    /// Adjust during runtime to optimize AGC behavior for varying audio dynamics.
    #[inline]
    pub fn get_release_coeff(&self) -> Arc<AtomicF32> {
        Arc::clone(&self.release_coeff)
    }

    #[cfg(feature = "experimental")]
    /// Access the AGC on/off control for real-time adjustment.
    ///
    /// Use this to dynamically enable or disable AGC processing during runtime.
    /// Useful for comparing processed and unprocessed audio or for disabling/enabling AGC at runtime.
    #[inline]
    pub fn get_agc_control(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.is_enabled)
    }

    #[cfg(not(feature = "experimental"))]
    /// Enable or disable AGC processing.
    ///
    /// Use this to enable or disable AGC processing.
    /// Useful for comparing processed and unprocessed audio or for disabling/enabling AGC.
    #[inline]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
    }

    /// Updates the peak level with an adaptive attack coefficient
    ///
    /// This method adjusts the peak level using a variable attack coefficient.
    /// It responds faster to sudden increases in signal level by using a
    /// minimum attack coefficient of `min_attack_coeff` when the sample value exceeds the
    /// current peak level. This adaptive behavior helps capture transients
    /// more accurately while maintaining smoother behavior for gradual changes.
    #[inline]
    fn update_peak_level(&mut self, sample_value: f32) {
        let attack_coeff = if sample_value > self.peak_level {
            self.attack_coeff().min(self.min_attack_coeff) // User-defined attack time limited via release_time
        } else {
            self.release_coeff()
        };

        self.peak_level = attack_coeff * self.peak_level + (1.0 - attack_coeff) * sample_value;
    }

    /// Updates the RMS (Root Mean Square) level using a circular buffer approach.
    /// This method calculates a moving average of the squared input samples,
    /// providing a measure of the signal's average power over time.
    #[inline]
    fn update_rms(&mut self, sample_value: f32) -> f32 {
        let squared_sample = sample_value * sample_value;
        self.rms_window.push(squared_sample);
        self.rms_window.mean().sqrt()
    }

    /// Calculate gain adjustments based on peak levels
    /// This method determines the appropriate gain level to apply to the audio
    /// signal, considering the peak level.
    /// The peak level helps prevent sudden spikes in the output signal.
    #[inline]
    fn calculate_peak_gain(&self) -> f32 {
        if self.peak_level > 0.0 {
            (self.target_level() / self.peak_level).min(self.absolute_max_gain())
        } else {
            self.absolute_max_gain()
        }
    }

    #[inline]
    fn process_sample(&mut self, sample: I::Item) -> I::Item {
        // Convert the sample to its absolute float value for level calculations
        let sample_value = sample.to_f32().abs();

        // Dynamically adjust peak level using an adaptive attack coefficient
        self.update_peak_level(sample_value);

        // Calculate the current RMS (Root Mean Square) level using a sliding window approach
        let rms = self.update_rms(sample_value);

        // Compute the gain adjustment required to reach the target level based on RMS
        let rms_gain = if rms > 0.0 {
            self.target_level() / rms
        } else {
            self.absolute_max_gain() // Default to max gain if RMS is zero
        };

        // Calculate the peak limiting gain
        let peak_gain = self.calculate_peak_gain();

        // Use RMS for general adjustments, but limit by peak gain to prevent clipping
        let desired_gain = rms_gain.min(peak_gain);

        // Adaptive attack/release speed for AGC (Automatic Gain Control)
        //
        // This mechanism implements an asymmetric approach to gain adjustment:
        // 1. **Slow increase**: Prevents abrupt amplification of noise during quiet periods.
        // 2. **Fast decrease**: Rapidly attenuates sudden loud signals to avoid distortion.
        //
        // The asymmetry is crucial because:
        // - Gradual gain increases sound more natural and less noticeable to listeners.
        // - Quick gain reductions are necessary to prevent clipping and maintain audio quality.
        //
        // This approach addresses several challenges associated with high attack times:
        // 1. **Slow response**: With a high attack time, the AGC responds very slowly to changes in input level.
        //    This means it takes longer for the gain to adjust to new signal levels.
        // 2. **Initial gain calculation**: When the audio starts or after a period of silence, the initial gain
        //    calculation might result in a very high gain value, especially if the input signal starts quietly.
        // 3. **Overshooting**: As the gain slowly increases (due to the high attack time), it might overshoot
        //    the desired level, causing the signal to become too loud.
        // 4. **Overcorrection**: The AGC then tries to correct this by reducing the gain, but due to the slow response,
        //    it might reduce the gain too much, causing the sound to drop to near-zero levels.
        // 5. **Slow recovery**: Again, due to the high attack time, it takes a while for the gain to increase
        //    back to the appropriate level.
        //
        // By using a faster release time for decreasing gain, we can mitigate these issues and provide
        // more responsive control over sudden level increases while maintaining smooth gain increases.
        let attack_speed = if desired_gain > self.current_gain {
            self.attack_coeff()
        } else {
            self.release_coeff()
        };

        // Gradually adjust the current gain towards the desired gain for smooth transitions
        self.current_gain = self.current_gain * attack_speed + desired_gain * (1.0 - attack_speed);

        // Ensure the calculated gain stays within the defined operational range
        self.current_gain = self.current_gain.clamp(0.1, self.absolute_max_gain());

        // Output current gain value for developers to fine tune their inputs to automatic_gain_control
        #[cfg(feature = "tracing")]
        tracing::debug!("AGC gain: {}", self.current_gain,);

        // Apply the computed gain to the input sample and return the result
        sample.amplify(self.current_gain)
    }

    /// Returns a mutable reference to the inner source.
    pub fn inner(&self) -> &I {
        &self.input
    }

    /// Returns the inner source.
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.input
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
        self.input.next().map(|sample| {
            if self.is_enabled() {
                self.process_sample(sample)
            } else {
                sample
            }
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
