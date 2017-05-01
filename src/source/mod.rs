//! Sources of sound and various filters.

use std::time::Duration;

use Sample;

pub use self::amplify::Amplify;
pub use self::buffered::Buffered;
pub use self::delay::Delay;
pub use self::empty::Empty;
pub use self::fadein::FadeIn;
pub use self::mix::Mix;
pub use self::pausable::Pausable;
pub use self::repeat::Repeat;
pub use self::samples_converter::SamplesConverter;
pub use self::sine::SineWave;
pub use self::speed::Speed;
pub use self::stoppable::Stoppable;
pub use self::take::TakeDuration;
pub use self::uniform::UniformSourceIterator;
pub use self::volume_filter::VolumeFilter;
pub use self::zero::Zero;

mod amplify;
mod buffered;
mod delay;
mod empty;
mod fadein;
mod mix;
mod pausable;
mod repeat;
mod samples_converter;
mod sine;
mod speed;
mod stoppable;
mod take;
mod uniform;
mod volume_filter;
mod zero;

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
/// - The number of channels can be retreived with `channels`.
/// - The frequency can be retreived with `samples_rate`.
/// - The list of values can be retreived by iterating on the source. The `Source` trait requires
///   that the `Iterator` trait be implemented as well.
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
/// `samples_rate` too frequently.
///
/// In order to properly handle this situation, the `current_frame_len()` method should return
/// the number of samples that remain in the iterator before the samples rate and number of
/// channels can potentially change.
///
pub trait Source: Iterator
    where Self::Item: Sample
{
    /// Returns the number of samples before the current frame ends. `None` means "infinite" or
    /// "until the sound ends".
    /// Should never return 0 unless there's no more data.
    ///
    /// After the engine has finished reading the specified number of samples, it will check
    /// whether the value of `channels()` and/or `samples_rate()` have changed.
    fn current_frame_len(&self) -> Option<usize>;

    /// Returns the number of channels. Channels are always interleaved.
    fn channels(&self) -> u16;

    /// Returns the rate at which the source should be played. In number of samples per second.
    fn samples_rate(&self) -> u32;

    /// Returns the total duration of this source, if known.
    ///
    /// `None` indicates at the same time "infinite" or "unknown".
    fn total_duration(&self) -> Option<Duration>;

    /// Stores the source in a buffer in addition to returning it. This iterator can be cloned.
    #[inline]
    fn buffered(self) -> Buffered<Self>
        where Self: Sized
    {
        buffered::buffered(self)
    }

    /// Mixes this source with another one.
    #[inline]
    fn mix<S>(self, other: S) -> Mix<Self, S>
        where Self: Sized,
              S: Source,
              S::Item: Sample
    {
        mix::mix(self, other)
    }

    /// Repeats this source forever.
    ///
    /// Note that this works by storing the data in a buffer, so the amount of memory used is
    /// proportional to the size of the sound.
    #[inline]
    fn repeat_infinite(self) -> Repeat<Self>
        where Self: Sized
    {
        repeat::repeat(self)
    }

    /// Takes a certain duration of this source and then stops.
    #[inline]
    fn take_duration(self, duration: Duration) -> TakeDuration<Self>
        where Self: Sized
    {
        take::take_duration(self, duration)
    }

    /// Delays the sound by a certain duration.
    ///
    /// The rate and channels of the silence will use the same format as the first frame of the
    /// source.
    #[inline]
    fn delay(self, duration: Duration) -> Delay<Self>
        where Self: Sized
    {
        delay::delay(self, duration)
    }

    /// Amplifies the sound by the given value.
    #[inline]
    fn amplify(self, value: f32) -> Amplify<Self>
        where Self: Sized
    {
        amplify::amplify(self, value)
    }

    /// Fades in the sound.
    #[inline]
    fn fade_in(self, duration: Duration) -> FadeIn<Self>
        where Self: Sized
    {
        fadein::fadein(self, duration)
    }

    /// Changes the play speed of the sound. Does not adjust the samples, only the play speed.
    #[inline]
    fn speed(self, ratio: f32) -> Speed<Self>
        where Self: Sized
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
        where Self: Sized + Clone
    {
        let echo = self.clone().amplify(amplitude).delay(duration);
        self.mix(echo)
    }

    /// Converts the samples of this source to another type.
    #[inline]
    fn convert_samples<D>(self) -> SamplesConverter<Self, D>
        where Self: Sized, D: Sample
    {
        SamplesConverter::new(self)
    }
}
