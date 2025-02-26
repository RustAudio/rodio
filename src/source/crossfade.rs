use crate::source::{FadeIn, Mix, TakeDuration};
use crate::Source;
use std::time::Duration;

/// Mixes one sound fading out with another sound fading in for the given
/// duration.
///
/// Only the crossfaded portion (beginning of fadeout, beginning of fadein) is
/// returned.
pub(crate) fn crossfade<I1, I2>(
    input_fadeout: I1,
    input_fadein: I2,
    duration: Duration,
) -> Crossfade<I1, I2>
where
    I1: Source,
    I2: Source,
{
    let mut input_fadeout = input_fadeout.take_duration(duration);
    input_fadeout.set_filter_fadeout();
    let input_fadein = input_fadein.take_duration(duration).fade_in(duration);
    input_fadeout.mix(input_fadein)
}

/// Mixes one sound fading out with another sound fading in for the given
/// duration.
///
/// Only the crossfaded portion (beginning of fadeout, beginning of fadein) is
/// covered.
pub type Crossfade<I1, I2> = Mix<TakeDuration<I1>, FadeIn<TakeDuration<I2>>>;
