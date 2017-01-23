use std::time::Duration;

use Sample;

pub use self::amplify::Amplify;
pub use self::buffered::Buffered;
pub use self::delay::Delay;
pub use self::fadein::FadeIn;
pub use self::mix::Mix;
pub use self::repeat::Repeat;
pub use self::sine::SineWave;
pub use self::speed::Speed;
pub use self::take::TakeDuration;
pub use self::uniform::UniformSourceIterator;

mod amplify;
mod buffered;
mod delay;
mod fadein;
mod mix;
mod repeat;
mod sine;
mod speed;
mod take;
mod uniform;

/// A source of samples.
pub trait Source: Iterator
    where Self::Item: Sample
{
    /// Returns the number of samples before the current frame ends. `None` means "infinite".
    /// Should never return 0 unless there's no more data.
    ///
    /// After the engine has finished reading the specified number of samples, it will assume that
    /// the value of `get_channels()` and/or `get_samples_rate()` have changed.
    fn get_current_frame_len(&self) -> Option<usize>;

    /// Returns the number of channels. Channels are always interleaved.
    fn get_channels(&self) -> u16;

    /// Returns the rate at which the source should be played.
    fn get_samples_rate(&self) -> u32;

    /// Returns the total duration of this source, if known.
    ///
    /// `None` indicates at the same time "infinite" or "unknown".
    fn get_total_duration(&self) -> Option<Duration>;

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
}
