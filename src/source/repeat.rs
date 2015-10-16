use std::time::Duration;
use std::mem;

use Sample;
use Source;

/// Internal function that builds a `Repeat` object.
pub fn repeat<I>(input: I) -> Repeat<I> where I: Source, I::Item: Sample {
    let buffer = vec![(Vec::new(), input.get_samples_rate(), input.get_channels())];
    Repeat { inner: RepeatImpl::FirstPass(input, buffer) }
}

/// A source that repeats the given source.
pub struct Repeat<I> where I: Source, I::Item: Sample {
    inner: RepeatImpl<I>,
}

enum RepeatImpl<I> where I: Source, I::Item: Sample {
    FirstPass(I, Vec<(Vec<I::Item>, u32, u16)>),
    NextPasses(Vec<(Vec<I::Item>, u32, u16)>, usize, usize)
}

impl<I> Iterator for Repeat<I> where I: Source, I::Item: Sample {
    type Item = <I as Iterator>::Item;

    #[inline]
    fn next(&mut self) -> Option<<I as Iterator>::Item> {
        match self.inner {
            RepeatImpl::FirstPass(ref mut input, ref mut buffer) => {
                match input.get_current_frame_len() {
                    Some(1) => {
                        if let Some(sample) = input.next() {
                            buffer.last_mut().unwrap().0.push(sample);
                            buffer.push((Vec::new(), input.get_samples_rate(), input.get_channels()));
                            return Some(sample);
                        }
                    },

                    Some(0) => {

                    },

                    _ => {
                        if let Some(sample) = input.next() {
                            buffer.last_mut().unwrap().0.push(sample);
                            return Some(sample);
                        }
                    },
                }
            },

            RepeatImpl::NextPasses(ref buffer, ref mut off1, ref mut off2) => {
                let sample = buffer[*off1].0[*off2];
                *off2 += 1;
                if *off2 >= buffer[*off1].0.len() {
                    *off1 += 1;
                    *off2 = 0;
                }
                if *off1 >= buffer.len() {
                    *off1 = 0;
                }
                return Some(sample);
            },
        }

        // if we reach this, we need to switch from FirstPass to NextPasses
        let buffer = if let RepeatImpl::FirstPass(_, ref mut buffer) = self.inner {
            mem::replace(buffer, Vec::new())
        } else {
            unreachable!()
        };

        mem::replace(&mut self.inner, RepeatImpl::NextPasses(buffer, 0, 0));
        self.next()
    }

    // TODO: size_hint
}

impl<I> Source for Repeat<I> where I: Iterator + Source, I::Item: Sample {
    #[inline]
    fn get_current_frame_len(&self) -> Option<usize> {
        match self.inner {
            RepeatImpl::FirstPass(ref input, _) => input.get_current_frame_len(),
            RepeatImpl::NextPasses(ref buffers, off1, off2) => Some(buffers[off1].0.len() - off2),
        }
    }

    #[inline]
    fn get_channels(&self) -> u16 {
        match self.inner {
            RepeatImpl::FirstPass(ref input, _) => input.get_channels(),
            RepeatImpl::NextPasses(ref buffers, off1, _) => buffers[off1].2,
        }
    }

    #[inline]
    fn get_samples_rate(&self) -> u32 {
        match self.inner {
            RepeatImpl::FirstPass(ref input, _) => input.get_samples_rate(),
            RepeatImpl::NextPasses(ref buffers, off1, _) => buffers[off1].1,
        }
    }

    #[inline]
    fn get_total_duration(&self) -> Option<Duration> {
        // TODO: ?
        None
    }
}
