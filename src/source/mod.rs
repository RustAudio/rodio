//! Sources of sound and various filters.

use core::fmt;
use core::time::Duration;

use cpal::FromSample;

use crate::Sample;

pub use self::agc::AutomaticGainControl;
pub use self::amplify::Amplify;
pub use self::blt::BltFilter;
pub use self::buffered::Buffered;
pub use self::channel_volume::ChannelVolume;
pub use self::chirp::{chirp, Chirp};
pub use self::crossfade::Crossfade;
pub use self::delay::Delay;
pub use self::done::Done;
pub use self::empty::Empty;
pub use self::empty_callback::EmptyCallback;
pub use self::fadein::FadeIn;
pub use self::fadeout::FadeOut;
pub use self::from_factory::{from_factory, FromFactoryIter};
pub use self::from_iter::{from_iter, FromIter};
pub use self::linear_ramp::LinearGainRamp;
pub use self::mix::Mix;
pub use self::pausable::Pausable;
pub use self::periodic::PeriodicAccess;
pub use self::position::TrackPosition;
pub use self::repeat::Repeat;
pub use self::samples_converter::SamplesConverter;
pub use self::signal_generator::{Function, SignalGenerator};
pub use self::sine::SineWave;
pub use self::skip::SkipDuration;
pub use self::skippable::Skippable;
pub use self::spatial::Spatial;
pub use self::speed::Speed;
pub use self::stoppable::Stoppable;
pub use self::take::TakeDuration;
pub use self::uniform::UniformSourceIterator;
pub use self::zero::Zero;

mod agc;
mod amplify;
mod blt;
mod buffered;
mod channel_volume;
mod chirp;
mod crossfade;
mod delay;
mod done;
mod empty;
mod empty_callback;
mod fadein;
mod fadeout;
mod from_factory;
mod from_iter;
mod linear_ramp;
mod mix;
mod pausable;
mod periodic;
mod position;
mod repeat;
mod samples_converter;
mod signal_generator;
mod sine;
mod skip;
mod skippable;
mod spatial;
mod speed;
mod stoppable;
mod take;
mod uniform;
mod zero;

#[cfg(feature = "noise")]
mod noise;
#[cfg(feature = "noise")]
pub use self::noise::{pink, white, PinkNoise, WhiteNoise};

/// A source of samples.
///
/// # A quick lesson about sounds
///
/// ## Sampling
///
/// A sound is a vibration that propagates through air and reaches your ears. This vibration can
/// be represented as an analog signal.
///
/// In order to store this signal in the computer's memory or on the disk, we perform what is
/// called *sampling*. The consists in choosing an interval of time (for example 20µs) and reading
/// the amplitude of the signal at each interval (for example, if the interval is 20µs we read the
/// amplitude every 20µs). By doing so we obtain a list of numerical values, each value being
/// called a *sample*.
///
/// Therefore a sound can be represented in memory by a frequency and a list of samples. The
/// frequency is expressed in hertz and corresponds to the number of samples that have been
/// read per second. For example if we read one sample every 20µs, the frequency would be
/// 50000 Hz. In reality, common values for the frequency are 44100, 48000 and 96000.
///
/// ## Channels
///
/// But a frequency and a list of values only represent one signal. When you listen to a sound,
/// your left and right ears don't receive exactly the same signal. In order to handle this,
/// we usually record not one but two different signals: one for the left ear and one for the right
/// ear. We say that such a sound has two *channels*.
///
/// Sometimes sounds even have five or six channels, each corresponding to a location around the
/// head of the listener.
///
/// The standard in audio manipulation is to *interleave* the multiple channels. In other words,
/// in a sound with two channels the list of samples contains the first sample of the first
/// channel, then the first sample of the second channel, then the second sample of the first
/// channel, then the second sample of the second channel, and so on. The same applies if you have
/// more than two channels. The rodio library only supports this schema.
///
/// Therefore in order to represent a sound in memory in fact we need three characteristics: the
/// frequency, the number of channels, and the list of samples.
///
/// ## The `Source` trait
///
/// A Rust object that represents a sound should implement the `Source` trait.
///
/// The three characteristics that describe a sound are provided through this trait:
///
/// - The number of channels can be retrieved with `channels`.
/// - The frequency can be retrieved with `sample_rate`.
/// - The list of values can be retrieved by iterating on the source. The `Source` trait requires
///   that the `Iterator` trait be implemented as well. When a `Source` returns None the
///   sound has ended.
///
/// # Frames
///
/// The samples rate and number of channels of some sound sources can change by itself from time
/// to time.
///
/// > **Note**: As a basic example, if you play two audio files one after the other and treat the
/// > whole as a single source, then the channels and samples rate of that source may change at the
/// > transition between the two files.
///
/// However, for optimization purposes rodio supposes that the number of channels and the frequency
/// stay the same for long periods of time and avoids calling `channels()` and
/// `sample_rate` too frequently.
///
/// In order to properly handle this situation, the `current_frame_len()` method should return
/// the number of samples that remain in the iterator before the samples rate and number of
/// channels can potentially change.
///
pub trait Source: Iterator
where
    Self::Item: Sample,
{
    /// Returns the number of samples before the current frame ends. `None` means "infinite" or
    /// "until the sound ends".
    /// Should never return 0 unless there's no more data.
    ///
    /// After the engine has finished reading the specified number of samples, it will check
    /// whether the value of `channels()` and/or `sample_rate()` have changed.
    fn current_frame_len(&self) -> Option<usize>;

    /// Returns the number of channels. Channels are always interleaved.
    fn channels(&self) -> u16;

    /// Returns the rate at which the source should be played. In number of samples per second.
    fn sample_rate(&self) -> u32;

    /// Returns the total duration of this source, if known.
    ///
    /// `None` indicates at the same time "infinite" or "unknown".
    fn total_duration(&self) -> Option<Duration>;

    /// Stores the source in a buffer in addition to returning it. This iterator can be cloned.

    #[inline]
    fn buffered(self) -> Buffered<Self>
    where
        Self: Sized,
    {
        buffered::buffered(self)
    }

    /// Mixes this source with another one.
    #[inline]
    fn mix<S>(self, other: S) -> Mix<Self, S>
    where
        Self: Sized,
        Self::Item: FromSample<S::Item>,
        S: Source,
        S::Item: Sample,
    {
        mix::mix(self, other)
    }

    /// Repeats this source forever.
    ///
    /// Note that this works by storing the data in a buffer, so the amount of memory used is
    /// proportional to the size of the sound.
    #[inline]
    fn repeat_infinite(self) -> Repeat<Self>
    where
        Self: Sized,
    {
        repeat::repeat(self)
    }

    /// Takes a certain duration of this source and then stops.
    #[inline]
    fn take_duration(self, duration: Duration) -> TakeDuration<Self>
    where
        Self: Sized,
    {
        take::take_duration(self, duration)
    }

    /// Delays the sound by a certain duration.
    ///
    /// The rate and channels of the silence will use the same format as the first frame of the
    /// source.
    #[inline]
    fn delay(self, duration: Duration) -> Delay<Self>
    where
        Self: Sized,
    {
        delay::delay(self, duration)
    }

    /// Immediately skips a certain duration of this source.
    ///
    /// If the specified duration is longer than the source itself, `skip_duration` will skip to the end of the source.
    #[inline]
    fn skip_duration(self, duration: Duration) -> SkipDuration<Self>
    where
        Self: Sized,
    {
        skip::skip_duration(self, duration)
    }

    /// Amplifies the sound by the given value.
    #[inline]
    fn amplify(self, value: f32) -> Amplify<Self>
    where
        Self: Sized,
    {
        amplify::amplify(self, value)
    }

    /// Applies automatic gain control to the sound.
    ///
    /// Automatic Gain Control (AGC) adjusts the amplitude of the audio signal
    /// to maintain a consistent output level.
    ///
    /// # Parameters
    ///
    /// `target_level`:
    ///   **TL;DR**: Desired output level. 1.0 = original level, > 1.0 amplifies, < 1.0 reduces.
    ///
    ///   The desired output level, where 1.0 represents the original sound level.
    ///   Values above 1.0 will amplify the sound, while values below 1.0 will lower it.
    ///   For example, a target_level of 1.4 means that at normal sound levels, the AGC
    ///   will aim to increase the gain by a factor of 1.4, resulting in a minimum 40% amplification.
    ///   A recommended level is `1.0`, which maintains the original sound level.
    ///
    /// `attack_time`:
    ///   **TL;DR**: Response time for volume increases. Shorter = faster but may cause abrupt changes. **Recommended: `4.0` seconds**.
    ///
    ///   The time (in seconds) for the AGC to respond to input level increases.
    ///   Shorter times mean faster response but may cause abrupt changes. Longer times result
    ///   in smoother transitions but slower reactions to sudden volume changes. Too short can
    ///   lead to overreaction to peaks, causing unnecessary adjustments. Too long can make the
    ///   AGC miss important volume changes or react too slowly to sudden loud passages. Very
    ///   high values might result in excessively loud output or sluggish response, as the AGC's
    ///   adjustment speed is limited by the attack time. Balance is key for optimal performance.
    ///   A recommended attack_time of `4.0` seconds provides a sweet spot for most applications.
    ///
    /// `release_time`:
    ///   **TL;DR**: Response time for volume decreases. Shorter = faster gain reduction. **Recommended: `0.005` seconds**.
    ///
    ///   The time (in seconds) for the AGC to respond to input level decreases.
    ///   This parameter controls how quickly the gain is reduced when the signal level drops.
    ///   Shorter release times result in faster gain reduction, which can be useful for quick
    ///   adaptation to quieter passages but may lead to pumping effects. Longer release times
    ///   provide smoother transitions but may be slower to respond to sudden decreases in volume.
    ///   However, if the release_time is too high, the AGC may not be able to lower the gain
    ///   quickly enough, potentially leading to clipping and distorted sound before it can adjust.
    ///   Finding the right balance is crucial for maintaining natural-sounding dynamics and
    ///   preventing distortion. A recommended release_time of `0.005` seconds often works well for
    ///   general use, providing a good balance between responsiveness and smooth transitions.
    ///
    /// `absolute_max_gain`:
    ///   **TL;DR**: Maximum allowed gain. Prevents over-amplification. **Recommended: `5.0`**.
    ///
    ///   The maximum gain that can be applied to the signal.
    ///   This parameter acts as a safeguard against excessive amplification of quiet signals
    ///   or background noise. It establishes an upper boundary for the AGC's signal boost,
    ///   effectively preventing distortion or overamplification of low-level sounds.
    ///   This is crucial for maintaining audio quality and preventing unexpected volume spikes.
    ///   A recommended value for `absolute_max_gain` is `5`, which provides a good balance between
    ///   amplification capability and protection against distortion in most scenarios.
    ///
    /// Use `get_agc_control` to obtain a handle for real-time enabling/disabling of the AGC.
    ///
    /// # Example (Quick start)
    ///
    /// ```rust
    /// // Apply Automatic Gain Control to the source (AGC is on by default)
    /// let agc_source = source.automatic_gain_control(1.0, 4.0, 0.005, 5.0);
    ///
    /// // Get a handle to control the AGC's enabled state (optional)
    /// let agc_control = agc_source.get_agc_control();
    ///
    /// // You can toggle AGC on/off at any time (optional)
    /// agc_control.store(false, std::sync::atomic::Ordering::Relaxed);
    ///
    /// // Add the AGC-controlled source to the sink
    /// sink.append(agc_source);
    ///
    /// // Note: Using agc_control is optional. If you don't need to toggle AGC,
    /// // you can simply use the agc_source directly without getting agc_control.
    /// ```
    #[inline]
    fn automatic_gain_control(
        self,
        target_level: f32,
        attack_time: f32,
        release_time: f32,
        absolute_max_gain: f32,
    ) -> AutomaticGainControl<Self>
    where
        Self: Sized,
    {
        // Added Limits to prevent the AGC from blowing up. ;)
        const MIN_ATTACK_TIME: f32 = 10.0;
        const MIN_RELEASE_TIME: f32 = 10.0;
        let attack_time = attack_time.min(MIN_ATTACK_TIME);
        let release_time = release_time.min(MIN_RELEASE_TIME);

        agc::automatic_gain_control(
            self,
            target_level,
            attack_time,
            release_time,
            absolute_max_gain,
        )
    }

    /// Mixes this sound fading out with another sound fading in for the given duration.
    ///
    /// Only the crossfaded portion (beginning of self, beginning of other) is returned.
    #[inline]
    fn take_crossfade_with<S: Source>(self, other: S, duration: Duration) -> Crossfade<Self, S>
    where
        Self: Sized,
        Self::Item: FromSample<S::Item>,
        <S as Iterator>::Item: Sample,
    {
        crossfade::crossfade(self, other, duration)
    }

    /// Fades in the sound.
    #[inline]
    fn fade_in(self, duration: Duration) -> FadeIn<Self>
    where
        Self: Sized,
    {
        fadein::fadein(self, duration)
    }

    /// Fades out the sound.
    #[inline]
    fn fade_out(self, duration: Duration) -> FadeOut<Self>
    where
        Self: Sized,
    {
        fadeout::fadeout(self, duration)
    }

    /// Applies a linear gain ramp to the sound.
    ///
    /// If `clamp_end` is `true`, all samples subsequent to the end of the ramp
    /// will be scaled by the `end_value`. If `clamp_end` is `false`, all
    /// subsequent samples will not have any scaling applied.
    #[inline]
    fn linear_gain_ramp(
        self,
        duration: Duration,
        start_value: f32,
        end_value: f32,
        clamp_end: bool,
    ) -> LinearGainRamp<Self>
    where
        Self: Sized,
    {
        linear_ramp::linear_gain_ramp(self, duration, start_value, end_value, clamp_end)
    }

    /// Calls the `access` closure on `Self` the first time the source is iterated and every
    /// time `period` elapses.
    ///
    /// Later changes in either `sample_rate()` or `channels_count()` won't be reflected in
    /// the rate of access.
    ///
    /// The rate is based on playback speed, so both the following will call `access` when the
    /// same samples are reached:
    /// `periodic_access(Duration::from_secs(1), ...).speed(2.0)`
    /// `speed(2.0).periodic_access(Duration::from_secs(2), ...)`
    #[inline]
    fn periodic_access<F>(self, period: Duration, access: F) -> PeriodicAccess<Self, F>
    where
        Self: Sized,
        F: FnMut(&mut Self),
    {
        periodic::periodic(self, period, access)
    }

    /// Changes the play speed of the sound. Does not adjust the samples, only the playback speed.
    ///
    /// # Note:
    /// 1. **Increasing the speed will increase the pitch by the same factor**
    /// - If you set the speed to 0.5 this will halve the frequency of the sound
    ///   lowering its pitch.
    /// - If you set the speed to 2 the frequency will double raising the
    ///   pitch of the sound.
    /// 2. **Change in the speed affect the total duration inversely**
    /// - If you set the speed to 0.5, the total duration will be twice as long.
    /// - If you set the speed to 2 the total duration will be halve of what it
    ///   was.
    ///
    /// See [`Speed`] for details
    #[inline]
    fn speed(self, ratio: f32) -> Speed<Self>
    where
        Self: Sized,
    {
        speed::speed(self, ratio)
    }

    /// Adds a basic reverb effect.
    ///
    /// This function requires the source to implement `Clone`. This can be done by using
    /// `buffered()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::time::Duration;
    ///
    /// let source = source.buffered().reverb(Duration::from_millis(100), 0.7);
    /// ```
    #[inline]
    fn reverb(self, duration: Duration, amplitude: f32) -> Mix<Self, Delay<Amplify<Self>>>
    where
        Self: Sized + Clone,
    {
        let echo = self.clone().amplify(amplitude).delay(duration);
        self.mix(echo)
    }

    /// Converts the samples of this source to another type.
    #[inline]
    fn convert_samples<D>(self) -> SamplesConverter<Self, D>
    where
        Self: Sized,
        D: Sample,
    {
        SamplesConverter::new(self)
    }

    /// Makes the sound pausable.
    // TODO: add example
    #[inline]
    fn pausable(self, initially_paused: bool) -> Pausable<Self>
    where
        Self: Sized,
    {
        pausable::pausable(self, initially_paused)
    }

    /// Makes the sound stoppable.
    // TODO: add example
    #[inline]
    fn stoppable(self) -> Stoppable<Self>
    where
        Self: Sized,
    {
        stoppable::stoppable(self)
    }

    /// Adds a method [`Skippable::skip`] for skipping this source. Skipping
    /// makes Source::next() return None. Which in turn makes the Sink skip to
    /// the next source.
    fn skippable(self) -> Skippable<Self>
    where
        Self: Sized,
    {
        skippable::skippable(self)
    }

    /// Start tracking the elapsed duration since the start of the underlying
    /// source.
    ///
    /// If a speedup and or delay is applied after this that will not be reflected
    /// in the position returned by [`get_pos`](TrackPosition::get_pos).
    ///
    /// This can get confusing when using [`get_pos()`](TrackPosition::get_pos)
    /// together with [`Source::try_seek()`] as the latter does take all
    /// speedup's and delay's into account. Its recommended therefore to apply
    /// track_position after speedup's and delay's.
    fn track_position(self) -> TrackPosition<Self>
    where
        Self: Sized,
    {
        position::track_position(self)
    }

    /// Applies a low-pass filter to the source.
    /// **Warning**: Probably buggy.
    #[inline]
    fn low_pass(self, freq: u32) -> BltFilter<Self>
    where
        Self: Sized,
        Self: Source<Item = f32>,
    {
        blt::low_pass(self, freq)
    }

    /// Applies a high-pass filter to the source.
    #[inline]
    fn high_pass(self, freq: u32) -> BltFilter<Self>
    where
        Self: Sized,
        Self: Source<Item = f32>,
    {
        blt::high_pass(self, freq)
    }

    /// Applies a low-pass filter to the source while allowing the q (bandwidth) to be changed.
    #[inline]
    fn low_pass_with_q(self, freq: u32, q: f32) -> BltFilter<Self>
    where
        Self: Sized,
        Self: Source<Item = f32>,
    {
        blt::low_pass_with_q(self, freq, q)
    }

    /// Applies a high-pass filter to the source while allowing the q (bandwidth) to be changed.
    #[inline]
    fn high_pass_with_q(self, freq: u32, q: f32) -> BltFilter<Self>
    where
        Self: Sized,
        Self: Source<Item = f32>,
    {
        blt::high_pass_with_q(self, freq, q)
    }

    // There is no `can_seek()` method as it is impossible to use correctly. Between
    // checking if a source supports seeking and actually seeking the sink can
    // switch to a new source.

    /// Attempts to seek to a given position in the current source.
    ///
    /// As long as the duration of the source is known seek is guaranteed to saturate
    /// at the end of the source. For example given a source that reports a total duration
    /// of 42 seconds calling `try_seek()` with 60 seconds as argument will seek to
    /// 42 seconds.
    ///
    /// # Errors
    /// This function will return [`SeekError::NotSupported`] if one of the underlying
    /// sources does not support seeking.
    ///
    /// It will return an error if an implementation ran
    /// into one during the seek.
    ///
    /// Seeking beyond the end of a source might return an error if the total duration of
    /// the source is not known.
    #[allow(unused_variables)]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported {
            underlying_source: std::any::type_name::<Self>(),
        })
    }
}

// We might add decoders requiring new error types, without non_exhaustive
// this would break users builds
/// Occurs when try_seek fails because the underlying decoder has an error or
/// does not support seeking.
#[non_exhaustive]
#[derive(Debug)]
pub enum SeekError {
    /// One of the underlying sources does not support seeking
    NotSupported {
        /// The source that did not support seek
        underlying_source: &'static str,
    },
    #[cfg(feature = "symphonia")]
    /// The symphonia decoder ran into an issue
    SymphoniaDecoder(crate::decoder::symphonia::SeekError),
    #[cfg(feature = "wav")]
    /// The hound (wav) decoder ran into an issue
    HoundDecoder(std::io::Error),
    // Prefer adding an enum variant to using this. Its meant for end users their
    // own try_seek implementations
    /// Any other error probably in a custom Source
    Other(Box<dyn std::error::Error + Send>),
}
impl fmt::Display for SeekError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeekError::NotSupported { underlying_source } => {
                write!(
                    f,
                    "Seeking is not supported by source: {}",
                    underlying_source
                )
            }
            #[cfg(feature = "symphonia")]
            SeekError::SymphoniaDecoder(err) => write!(f, "Error seeking: {}", err),
            #[cfg(feature = "wav")]
            SeekError::HoundDecoder(err) => write!(f, "Error seeking in wav source: {}", err),
            SeekError::Other(_) => write!(f, "An error occurred"),
        }
    }
}
impl std::error::Error for SeekError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SeekError::NotSupported { .. } => None,
            #[cfg(feature = "symphonia")]
            SeekError::SymphoniaDecoder(err) => Some(err),
            #[cfg(feature = "wav")]
            SeekError::HoundDecoder(err) => Some(err),
            SeekError::Other(err) => Some(err.as_ref()),
        }
    }
}
impl From<crate::decoder::symphonia::SeekError> for SeekError {
    fn from(source: crate::decoder::symphonia::SeekError) -> Self {
        SeekError::SymphoniaDecoder(source)
    }
}

impl SeekError {
    /// Will the source remain playing at its position before the seek or is it
    /// broken?
    pub fn source_intact(&self) -> bool {
        match self {
            SeekError::NotSupported { .. } => true,
            #[cfg(feature = "symphonia")]
            SeekError::SymphoniaDecoder(_) => false,
            #[cfg(feature = "wav")]
            SeekError::HoundDecoder(_) => false,
            SeekError::Other(_) => false,
        }
    }
}

macro_rules! source_pointer_impl {
    ($($sig:tt)+) => {
        impl $($sig)+ {
            #[inline]
            fn current_frame_len(&self) -> Option<usize> {
                (**self).current_frame_len()
            }

            #[inline]
            fn channels(&self) -> u16 {
                (**self).channels()
            }

            #[inline]
            fn sample_rate(&self) -> u32 {
                (**self).sample_rate()
            }

            #[inline]
            fn total_duration(&self) -> Option<Duration> {
                (**self).total_duration()
            }

            #[inline]
            fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
                (**self).try_seek(pos)
            }
        }
    };
}

source_pointer_impl!(<S> Source for Box<dyn Source<Item = S>> where S: Sample,);

source_pointer_impl!(<S> Source for Box<dyn Source<Item = S> + Send> where S: Sample,);

source_pointer_impl!(<S> Source for Box<dyn Source<Item = S> + Send + Sync> where S: Sample,);

source_pointer_impl!(<'a, S, C> Source for &'a mut C where S: Sample, C: Source<Item = S>,);
