use std::time::Duration;

use crate::Sample;
use crate::Source;

/// Internal function that builds a `PeriodicAccess` object.
pub fn periodic<I, F>(source: I, period: Duration, modifier: F) -> PeriodicAccess<I, F>
where
    I: Source,
    I::Item: Sample,
{
    // TODO: handle the fact that the samples rate can change
    // TODO: generally, just wrong
    let update_ms = period.as_secs() as u32 * 1_000 + period.subsec_nanos() / 1_000_000;
    let update_frequency = (update_ms * source.sample_rate()) / 1000 * source.channels() as u32;

    PeriodicAccess {
        input: source,
        modifier: modifier,
        // Can overflow when subtracting if this is 0
        update_frequency: if update_frequency == 0 {
            1
        } else {
            update_frequency
        },
        samples_until_update: 1,
    }
}

/// Calls a function on a source every time a period elapsed.
#[derive(Clone, Debug)]
pub struct PeriodicAccess<I, F> {
    // The inner source.
    input: I,

    // Closure that gets access to `inner`.
    modifier: F,

    // The frequency with which local_volume should be updated by remote_volume
    update_frequency: u32,

    // How many samples remain until it is time to update local_volume with remote_volume.
    samples_until_update: u32,
}

impl<I, F> PeriodicAccess<I, F>
where
    I: Source,
    I::Item: Sample,
    F: FnMut(&mut I),
{
    /// Returns a reference to the inner source.
    #[inline]
    pub fn inner(&self) -> &I {
        &self.input
    }

    /// Returns a mutable reference to the inner source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.input
    }

    /// Returns the inner source.
    #[inline]
    pub fn into_inner(self) -> I {
        self.input
    }
}

impl<I, F> Iterator for PeriodicAccess<I, F>
where
    I: Source,
    I::Item: Sample,
    F: FnMut(&mut I),
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        self.samples_until_update -= 1;
        if self.samples_until_update == 0 {
            (self.modifier)(&mut self.input);
            self.samples_until_update = self.update_frequency;
        }

        self.input.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I, F> Source for PeriodicAccess<I, F>
where
    I: Source,
    I::Item: Sample,
    F: FnMut(&mut I),
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
}

#[cfg(test)]
mod tests {
    use crate::buffer::SamplesBuffer;
    use crate::source::Source;
    use std::cell::RefCell;
    use std::time::Duration;

    #[test]
    fn stereo_access() {
        // Stereo, 1Hz audio buffer
        let inner = SamplesBuffer::new(2, 1, vec![10i16, -10, 10, -10, 20, -20]);

        let cnt = RefCell::new(0);

        let mut source = inner.periodic_access(Duration::from_millis(1000), |_src| {
            *cnt.borrow_mut() += 1;
        });

        assert_eq!(*cnt.borrow(), 0);
        // Always called on first access!
        assert_eq!(source.next(), Some(10));
        assert_eq!(*cnt.borrow(), 1);
        // Called every 1 second afterwards
        assert_eq!(source.next(), Some(-10));
        assert_eq!(*cnt.borrow(), 1);
        assert_eq!(source.next(), Some(10));
        assert_eq!(*cnt.borrow(), 2);
        assert_eq!(source.next(), Some(-10));
        assert_eq!(*cnt.borrow(), 2);
        assert_eq!(source.next(), Some(20));
        assert_eq!(*cnt.borrow(), 3);
        assert_eq!(source.next(), Some(-20));
        assert_eq!(*cnt.borrow(), 3);
    }

    #[test]
    fn fast_access_overflow() {
        // 1hz is lower than 0.5 samples per 5ms
        let inner = SamplesBuffer::new(1, 1, vec![10i16, -10, 10, -10, 20, -20]);
        let mut source = inner.periodic_access(Duration::from_millis(5), |_src| {});

        source.next();
        source.next(); // Would overflow here.
    }
}
