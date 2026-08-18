pub mod fixed_source {
    use std::time::Duration;

    use crate::source::SeekError;
    use crate::{nz, FixedSource};
    use crate::{ChannelCount, SampleRate};

    /// A source producing an infinite amount of Silence. Like all generators you
    /// probably want to limit the duration of this source.
    ///
    /// # Example
    /// Padding a [`TakeDuration`](crate::fixed_source::Placeholder) to
    /// guarantee an exact playtime:
    ///
    /// ```rust,no_run
    /// # use std::time::Duration;
    /// # use rodio::nz;
    /// # use rodio::generators::fixed_source::Silence;
    /// # use rodio::{ConstSource, FixedSource};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let unknown_length = Silence::new(nz!(44100));
    /// let silence = Silence::new(nz!(44100));
    /// let two_seconds = unknown_length
    ///     .try_chain_source(silence)?
    ///     .take_duration(Duration::from_secs(2));
    /// # Ok(())
    /// # }
    /// ```
    pub struct Silence {
        sample_rate: SampleRate,
    }

    impl Silence {
        /// Create an infinite silence
        pub fn new(sample_rate: SampleRate) -> Self {
            Self { sample_rate }
        }
    }

    impl FixedSource for Silence {
        fn channels(&self) -> ChannelCount {
            nz!(1)
        }

        fn sample_rate(&self) -> SampleRate {
            self.sample_rate
        }

        fn total_duration(&self) -> Option<std::time::Duration> {
            None
        }

        /// This does nothing since all silence is equal :3
        fn try_seek(&mut self, _: Duration) -> Result<(), SeekError> {
            Ok(())
        }
    }

    impl Iterator for Silence {
        type Item = crate::Sample;

        fn next(&mut self) -> Option<Self::Item> {
            Some(0.0)
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            (usize::MAX, None)
        }
    }
}

pub mod const_source {
    use std::time::Duration;

    use crate::source::SeekError;
    use crate::ConstSource;

    /// A source producing an infinite amount of Silence. Like all generators you
    /// probably want to limit the duration of this source.
    ///
    /// # Example
    /// Padding a [`TakeDuration`](crate::const_source::Placeholder) to
    /// guarantee an exact playtime:
    ///
    /// ```rust,no_run
    /// # use std::time::Duration;
    /// # use rodio::nz;
    /// # use rodio::generators::const_source::Silence;
    /// # use rodio::ConstSource;
    /// # let unknown_length = Silence::new();
    ///
    /// let silence: Silence<44100> = Silence::new();
    /// let two_seconds = unknown_length
    ///     .chain_source(silence)
    ///     .take_duration(Duration::from_secs(2));
    /// ```
    pub struct Silence<const SR: u32>;

    impl<const SR: u32> Silence<SR> {
        /// Create an infinite silence
        pub fn new() -> Self {
            Self
        }
    }

    impl<const SR: u32> Default for Silence<SR> {
        fn default() -> Self {
            Self
        }
    }

    impl<const SR: u32> ConstSource<SR, 1> for Silence<SR> {
        fn total_duration(&self) -> Option<Duration> {
            None
        }

        /// This does nothing since all silence is equal :3
        fn try_seek(&mut self, _: Duration) -> Result<(), SeekError> {
            Ok(())
        }
    }

    impl<const SR: u32> Iterator for Silence<SR> {
        type Item = crate::Sample;

        fn next(&mut self) -> Option<Self::Item> {
            Some(0.0)
        }

        fn size_hint(&self) -> (usize, Option<usize>) {
            (usize::MAX, None)
        }
    }
}
