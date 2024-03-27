use cpal::Sample;

/// Iterator that converts from a certain channel count to another.
#[derive(Clone, Debug)]
pub struct ChannelCountConverter<I>
where
    I: Iterator,
{
    input: I,
    from: cpal::ChannelCount,
    to: cpal::ChannelCount,
    sample_repeat: Option<I::Item>,
    next_output_sample_pos: cpal::ChannelCount,
}

impl<I> ChannelCountConverter<I>
where
    I: Iterator,
{
    /// Initializes the iterator.
    ///
    /// # Panic
    ///
    /// Panics if `from` or `to` are equal to 0.
    ///
    #[inline]
    pub fn new(
        input: I,
        from: cpal::ChannelCount,
        to: cpal::ChannelCount,
    ) -> ChannelCountConverter<I> {
        assert!(from >= 1);
        assert!(to >= 1);

        ChannelCountConverter {
            input,
            from,
            to,
            sample_repeat: None,
            next_output_sample_pos: 0,
        }
    }

    /// Destroys this iterator and returns the underlying iterator.
    #[inline]
    pub fn into_inner(self) -> I {
        self.input
    }
}

impl<I> Iterator for ChannelCountConverter<I>
where
    I: Iterator,
    I::Item: Sample,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        let result = match self.next_output_sample_pos {
            0 => {
                // save first sample for mono -> stereo conversion
                let value = self.input.next();
                self.sample_repeat = value;
                value
            }
            x if x < self.from => self.input.next(),
            1 => self.sample_repeat,
            _ => Some(I::Item::EQUILIBRIUM),
        };

        self.next_output_sample_pos += 1;

        if self.next_output_sample_pos == self.to {
            self.next_output_sample_pos = 0;

            if self.from > self.to {
                for _ in self.to..self.from {
                    self.input.next(); // discarding extra input
                }
            }
        }

        result
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (min, max) = self.input.size_hint();

        let consumed = std::cmp::min(self.from, self.next_output_sample_pos) as usize;
        let calculate = |size| {
            (size + consumed) / self.from as usize * self.to as usize
                - self.next_output_sample_pos as usize
        };

        let min = calculate(min);
        let max = max.map(calculate);

        (min, max)
    }
}

impl<I> ExactSizeIterator for ChannelCountConverter<I>
where
    I: ExactSizeIterator,
    I::Item: Sample,
{
}

#[cfg(test)]
mod test {
    use super::ChannelCountConverter;

    #[test]
    fn remove_channels() {
        let input = vec![1u16, 2, 3, 4, 5, 6];
        let output = ChannelCountConverter::new(input.into_iter(), 3, 2).collect::<Vec<_>>();
        assert_eq!(output, [1, 2, 4, 5]);

        let input = vec![1u16, 2, 3, 4, 5, 6, 7, 8];
        let output = ChannelCountConverter::new(input.into_iter(), 4, 1).collect::<Vec<_>>();
        assert_eq!(output, [1, 5]);
    }

    #[test]
    fn add_channels() {
        let input = vec![1i16, 2, 3, 4];
        let output = ChannelCountConverter::new(input.into_iter(), 1, 2).collect::<Vec<_>>();
        assert_eq!(output, [1, 1, 2, 2, 3, 3, 4, 4]);

        let input = vec![1i16, 2];
        let output = ChannelCountConverter::new(input.into_iter(), 1, 4).collect::<Vec<_>>();
        assert_eq!(output, [1, 1, 0, 0, 2, 2, 0, 0]);

        let input = vec![1i16, 2, 3, 4];
        let output = ChannelCountConverter::new(input.into_iter(), 2, 4).collect::<Vec<_>>();
        assert_eq!(output, [1, 2, 0, 0, 3, 4, 0, 0]);
    }

    #[test]
    fn size_hint() {
        fn test(input: &[i16], from: cpal::ChannelCount, to: cpal::ChannelCount) {
            let input = input.to_vec();
            let scaled_len = input.len() / from as usize * to as usize;
            let mut converter = ChannelCountConverter::new(input.into_iter(), from, to);
            for i in 0..scaled_len {
                assert_eq!(
                    converter.size_hint(),
                    (scaled_len - i, Some(scaled_len - i))
                );
                converter.next();
            }
        }

        test(&[1i16, 2, 3], 1, 2);
        test(&[1i16, 2, 3, 4], 2, 4);
        test(&[1i16, 2, 3, 4], 4, 2);
        test(&[1i16, 2, 3, 4, 5, 6], 3, 8);
        test(&[1i16, 2, 3, 4, 5, 6, 7, 8], 4, 1);
    }

    #[test]
    fn len_more() {
        let input = vec![1i16, 2, 3, 4];
        let output = ChannelCountConverter::new(input.into_iter(), 2, 3);
        assert_eq!(output.len(), 6);
    }

    #[test]
    fn len_less() {
        let input = vec![1i16, 2, 3, 4];
        let output = ChannelCountConverter::new(input.into_iter(), 2, 1);
        assert_eq!(output.len(), 2);
    }
}
