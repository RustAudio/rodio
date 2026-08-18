macro_rules! source_impl {
    () => {
        /// # Panics
        /// If the length of the buffer is larger than approximately 16 billion elements.
        /// This is because the calculation of the duration would overflow.
        #[inline]
        fn total_duration(&self) -> Option<Duration> {
            use crate::math::NANOS_PER_SEC;

            let duration_ns = NANOS_PER_SEC
                .checked_mul(self.data.len() as u64)
                .expect("slices longer then 16 billion elements are not supported")
                / self.sample_rate().get() as u64
                / self.channels().get() as u64;
            let duration = Duration::new(
                duration_ns / NANOS_PER_SEC,
                (duration_ns % NANOS_PER_SEC) as u32,
            );

            Some(duration)
        }

        /// This jumps in memory to the sample corresponding to `pos`.
        #[inline]
        fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
            // This is fast because all the samples are in memory already
            // and due to the constant sample_rate we can jump to the right
            // sample directly.

            let curr_channel = self.pos % self.channels().get() as usize;
            let new_pos = crate::math::duration_to_float(pos)
                * self.sample_rate().get() as crate::Float
                * self.channels().get() as crate::Float;
            // saturate pos at the end of the source
            let new_pos = new_pos as usize;
            let new_pos = new_pos.min(self.data.len());

            // make sure the next sample is for the right channel
            let new_pos = new_pos.next_multiple_of(self.channels().get() as usize);
            let new_pos = new_pos - curr_channel;

            self.pos = new_pos;
            Ok(())
        }
    };
}

macro_rules! iter_impl {
    () => {
        type Item = Sample;
        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            let sample = self.data.get(self.pos)?;
            self.pos += 1;
            Some(*sample)
        }
        #[inline]
        fn size_hint(&self) -> (usize, Option<usize>) {
            let remaining = self.data.len() - self.pos;
            (remaining, Some(remaining))
        }
    };
}

pub(crate) use iter_impl;
pub(crate) use source_impl;
