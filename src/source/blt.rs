use std::f32::consts::PI;
use std::time::Duration;

use crate::Source;

// Implemented following http://www.musicdsp.org/files/Audio-EQ-Cookbook.txt

/// Internal function that builds a `BltFilter` object.
pub fn low_pass<I>(input: I, freq: u32) -> BltFilter<I>
where
    I: Source<Item = f32>,
{
    BltFilter {
        input: input,
        formula: BltFormula::LowPass { freq: freq, q: 0.5 },
        applier: None,
        x_n1: 0.0,
        x_n2: 0.0,
        y_n1: 0.0,
        y_n2: 0.0,
    }
}

#[derive(Clone, Debug)]
pub struct BltFilter<I> {
    input: I,
    formula: BltFormula,
    applier: Option<BltApplier>,
    x_n1: f32,
    x_n2: f32,
    y_n1: f32,
    y_n2: f32,
}

impl<I> BltFilter<I> {
    /// Modifies this filter so that it becomes a low-pass filter.
    pub fn to_low_pass(&mut self, freq: u32) {
        self.formula = BltFormula::LowPass { freq: freq, q: 0.5 };
        self.applier = None;
    }

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

impl<I> Iterator for BltFilter<I>
where
    I: Source<Item = f32>,
{
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        let last_in_frame = self.input.current_frame_len() == Some(1);

        if self.applier.is_none() {
            self.applier = Some(self.formula.to_applier(self.input.sample_rate()));
        }

        let sample = match self.input.next() {
            None => return None,
            Some(s) => s,
        };

        let result = self
            .applier
            .as_ref()
            .unwrap()
            .apply(sample, self.x_n1, self.x_n2, self.y_n1, self.y_n2);

        self.y_n2 = self.y_n1;
        self.x_n2 = self.x_n1;
        self.y_n1 = result;
        self.x_n1 = sample;

        if last_in_frame {
            self.applier = None;
        }

        Some(result)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<I> ExactSizeIterator for BltFilter<I> where I: Source<Item = f32> + ExactSizeIterator {}

impl<I> Source for BltFilter<I>
where
    I: Source<Item = f32>,
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

#[derive(Clone, Debug)]
enum BltFormula {
    LowPass { freq: u32, q: f32 },
}

impl BltFormula {
    fn to_applier(&self, sampling_frequency: u32) -> BltApplier {
        match self {
            &BltFormula::LowPass { freq, q } => {
                let w0 = 2.0 * PI * freq as f32 / sampling_frequency as f32;

                let alpha = w0.sin() / (2.0 * q);
                let b1 = 1.0 - w0.cos();
                let b0 = b1 / 2.0;
                let b2 = b0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * w0.cos();
                let a2 = 1.0 - alpha;

                BltApplier {
                    b0: b0 / a0,
                    b1: b1 / a0,
                    b2: b2 / a0,
                    a1: a1 / a0,
                    a2: a2 / a0,
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct BltApplier {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl BltApplier {
    #[inline]
    fn apply(&self, x_n: f32, x_n1: f32, x_n2: f32, y_n1: f32, y_n2: f32) -> f32 {
        self.b0 * x_n + self.b1 * x_n1 + self.b2 * x_n2 - self.a1 * y_n1 - self.a2 * y_n2
    }
}
