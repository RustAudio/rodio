use conversions::Sample;

pub struct AmplifierIterator<I> where I: Iterator {
    input: I,
    amplication: f32,
}

impl<I> AmplifierIterator<I> where I: Iterator {
    #[inline]
    pub fn new(input: I, amplication: f32) -> AmplifierIterator<I> {
        AmplifierIterator {
            input: input,
            amplication: amplication,
        }
    }

    #[inline]
    pub fn set_amplification(&mut self, new_value: f32) {
        self.amplication = new_value;
    }
}

impl<I> Iterator for AmplifierIterator<I> where I: Iterator, I::Item: Sample {
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        self.input.next().map(|value| value.amplify(self.amplication))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for AmplifierIterator<I>
                              where I: ExactSizeIterator, I::Item: Sample {}
