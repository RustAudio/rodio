pub mod fixed_source {
    use crate::{ChannelCount, SampleRate};
    use crate::{FixedSource, nz};

    /// A source producing an infinite amount of Silence. Like all generators you
    /// probably want to limit the duration of this source.
    ///
    /// # Example
    /// Padding a [`TakeDuration`](crate::effects::fixed_source::TakeDuration) to
    /// guarantee an exact playtime:
    ///
    /// ```rust,no_run
    /// # use std::time::Duration;
    /// # use rodio_experiments::nz;
    /// # use rodio_experiments::generators::fixed_source::function::SineWave;
    /// # use rodio_experiments::generators::fixed_source::Silence;
    /// # use rodio_experiments::ConstSource;
    /// # use rodio_experiments::fixed_source::FixedSourceExt;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let unknown_length = SineWave::new(440.0, nz!(44100));
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
    use crate::ConstSource;

    /// A source producing an infinite amount of Silence. Like all generators you
    /// probably want to limit the duration of this source.
    ///
    /// # Example
    /// Padding a [`TakeDuration`](crate::effects::const_source::TakeDuration) to
    /// guarantee an exact playtime:
    ///
    /// ```rust,no_run
    /// # use std::time::Duration;
    /// # use rodio_experiments::nz;
    /// # use rodio_experiments::generators::const_source::function::SineWave;
    /// # use rodio_experiments::generators::const_source::Silence;
    /// # use rodio_experiments::ConstSource;
    /// # use rodio_experiments::fixed_source::FixedSourceExt;
    /// # let unknown_length = SineWave::new(440.0);
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
        fn total_duration(&self) -> Option<std::time::Duration> {
            None
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
