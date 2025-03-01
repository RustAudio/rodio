/// Linear interpolation between two samples.
///
/// The result should be equivalent to
/// `first * (1 - numerator / denominator) + second * numerator / denominator`.
///
/// To avoid numeric overflows pick smaller numerator.
// TODO (refactoring) Streamline this using coefficient instead of numerator and denominator.
#[inline]
pub fn lerp(first: &f32, second: &f32, numerator: u32, denominator: u32) -> f32 {
    first + (second - first) * numerator as f32 / denominator as f32
}

/// will hopefully get stabilized, this is slightly different to the future
/// std's version since it does some casting already. When the std's version gets
/// stable remove this trait.
pub(crate) trait PrevMultipleOf {
    fn prev_multiple_of(self, n: u16) -> Self;
}

macro_rules! impl_prev_multiple_of {
    ($type:ty) => {
        impl PrevMultipleOf for $type {
            fn prev_multiple_of(self, n: u16) -> $type {
                if self.next_multiple_of(n as $type) > self {
                    self.next_multiple_of(n as $type) - n as $type
                } else {
                    self.next_multiple_of(n as $type)
                }
            }
        }
    };
}

impl_prev_multiple_of! {usize}
impl_prev_multiple_of! {u64}

#[cfg(test)]
mod test {
    use super::*;
    use num_rational::Ratio;
    use quickcheck::{quickcheck, TestResult};

    quickcheck! {
        fn lerp_f32_random(first: u16, second: u16, numerator: u16, denominator: u16) -> TestResult {
            if denominator == 0 { return TestResult::discard(); }

            let (numerator, denominator) = Ratio::new(numerator, denominator).into_raw();
            if numerator > 5000 { return TestResult::discard(); }

            let a = first as f64;
            let b = second as f64;
            let c = numerator as f64 / denominator as f64;
            if c < 0.0 || c > 1.0 { return TestResult::discard(); };

            let reference = a * (1.0 - c) + b * c;
            let x = lerp(&(first as f32), &(second as f32), numerator as u32, denominator as u32) as f64;
            // TODO (review) It seems that the diff tolerance should be a lot lower. Why lerp so imprecise?
            TestResult::from_bool((x - reference).abs() < 0.01)
        }
    }
}
