use std::time::Duration;

use cpal::FromSample;

use crate::source::{FadeIn, Mix, TakeDuration};
use crate::{Sample, Source};

/// Mixes one sound fading out with another sound fading in for the given duration.
///
/// Only the crossfaded portion (beginning of fadeout, beginning of fadein) is returned.
pub fn crossfade<I1, I2>(
    input_fadeout: I1,
    input_fadein: I2,
    duration: Duration,
) -> Crossfade<I1, I2>
where
    I1: Source,
    I2: Source,
    I1::Item: FromSample<I2::Item> + Sample,
    I2::Item: Sample,
{
    let mut input_fadeout = input_fadeout.take_duration(duration);
    input_fadeout.set_filter_fadeout();
    let input_fadein = input_fadein.take_duration(duration).fade_in(duration);
    input_fadeout.mix(input_fadein)
}

pub type Crossfade<I1, I2> = Mix<TakeDuration<I1>, FadeIn<TakeDuration<I2>>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::SamplesBuffer;
    fn dummysource(length: u8) -> SamplesBuffer<f32> {
        let data: Vec<f32> = (1..=length).map(f32::from).collect();
        SamplesBuffer::new(1, 1, data)
    }

    #[test]
    fn test_crossfade() {
        let source1 = dummysource(10);
        let source2 = dummysource(10);
        let mut mixed = crossfade(
            source1,
            source2,
            Duration::from_secs(5) + Duration::from_nanos(1),
        );
        assert_eq!(mixed.next(), Some(1.0));
        assert_eq!(mixed.next(), Some(2.0));
        assert_eq!(mixed.next(), Some(3.0));
        assert_eq!(mixed.next(), Some(4.0));
        assert_eq!(mixed.next(), Some(5.0));
        assert_eq!(mixed.next(), None);

        let source1 = dummysource(10);
        let source2 = dummysource(10).amplify(0.0);
        let mut mixed = crossfade(
            source1,
            source2,
            Duration::from_secs(5) + Duration::from_nanos(1),
        );
        assert_eq!(mixed.next(), Some(1.0 * 1.0));
        assert_eq!(mixed.next(), Some(2.0 * 0.8));
        assert_eq!(mixed.next(), Some(3.0 * 0.6));
        assert_eq!(mixed.next(), Some(4.0 * 0.4));
        assert_eq!(mixed.next(), Some(5.0 * 0.2));
        assert_eq!(mixed.next(), None);
    }
}
