use std::time::Duration;

use Sample;

pub use self::repeat::Repeat;
pub use self::uniform::UniformSourceIterator;

mod repeat;
mod uniform;

/// A source of samples.
pub trait Source: Iterator where Self::Item: Sample {
    /// Returns the number of samples before the current channel ends. `None` means "infinite".
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

    /// Repeats this source forever.
    ///
    /// Note that this works by storing the data in a buffer, so the amount of memory used is
    /// proportional to the size of the sound.
    #[inline]
    fn repeat_infinite(self) -> Repeat<Self> where Self: Sized {
        repeat::repeat(self)
    }
}
