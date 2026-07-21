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
    /// # use rodio::nz;
    /// # use rodio::generators::const_source::Silence;
    /// # use rodio::ConstSource;
    /// # let unknown_length = Silence::new();
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
