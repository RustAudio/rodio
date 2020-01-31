use cpal;

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
    /// Panicks if `from` or `to` are equal to 0.
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
            input: input,
            from: from,
            to: to,
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
    I::Item: Clone,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        let result = if self.next_output_sample_pos == self.from - 1 {
            let value = self.input.next();
            self.sample_repeat = value.clone();
            value
        } else if self.next_output_sample_pos < self.from {
            self.input.next()
        } else {
            self.sample_repeat.clone()
        };

        self.next_output_sample_pos += 1;

        if self.next_output_sample_pos == self.to {
            self.next_output_sample_pos -= self.to;

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

        let min =
            (min / self.from as usize) * self.to as usize + self.next_output_sample_pos as usize;
        let max = max.map(|max| {
            (max / self.from as usize) * self.to as usize + self.next_output_sample_pos as usize
        });

        (min, max)
    }
}

impl<I> ExactSizeIterator for ChannelCountConverter<I>
where
    I: ExactSizeIterator,
    I::Item: Clone,
{
}

#[cfg(test)]
mod test {
    use super::ChannelCountConverter;

    #[test]
    fn remove_channels() {
        let input = vec![1u16, 2, 3, 1, 2, 3];
        let output = ChannelCountConverter::new(input.into_iter(), 3, 2).collect::<Vec<_>>();
        assert_eq!(output, [1, 2, 1, 2]);

        let input = vec![1u16, 2, 3, 4, 1, 2, 3, 4];
        let output = ChannelCountConverter::new(input.into_iter(), 4, 1).collect::<Vec<_>>();
        assert_eq!(output, [1, 1]);
    }

    #[test]
    fn add_channels() {
        let input = vec![1u16, 2, 1, 2];
        let output = ChannelCountConverter::new(input.into_iter(), 2, 3).collect::<Vec<_>>();
        assert_eq!(output, [1, 2, 2, 1, 2, 2]);

        let input = vec![1u16, 2, 1, 2];
        let output = ChannelCountConverter::new(input.into_iter(), 2, 4).collect::<Vec<_>>();
        assert_eq!(output, [1, 2, 2, 2, 1, 2, 2, 2]);
    }

    #[test]
    fn len_more() {
        let input = vec![1u16, 2, 1, 2];
        let output = ChannelCountConverter::new(input.into_iter(), 2, 3).collect::<Vec<_>>();
        assert_eq!(output.len(), 6);
    }

    #[test]
    fn len_less() {
        let input = vec![1u16, 2, 1, 2];
        let output = ChannelCountConverter::new(input.into_iter(), 2, 1).collect::<Vec<_>>();
        assert_eq!(output.len(), 2);
    }
}
