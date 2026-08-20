use crate::math::{db_to_linear, normalized_to_linear};

use crate::effects::pure_effect;

/// Amplification factor.
///
/// Recommended for volume control: [`Normalized`](Factor::Normalized).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Factor {
    /// Makes the sound exactly this times louder or softer. Note human hearing
    /// is logarithmic so something that is N times louder does not sound N
    /// times louder.
    Linear(f32),
    /// Amplifies the sound logarithmically by the given value.
    ///   - 0 dB = linear value of 1.0 (no change)
    ///   - Positive dB values represent amplification (> 1.0)
    ///   - Negative dB values represent attenuation (< 1.0)
    ///   - -60 dB ≈ 0.001 (barely audible)
    ///   - +20 dB = 10.0 (10x amplification)
    ///
    Decibel(f32),
    /// Normalized amplification in `[0.0, 1.0]` range. This method better
    /// matches the perceived loudness of sounds in human hearing and is
    /// recommended to use when you want to change volume in `[0.0, 1.0]` range.
    /// based on article: <https://www.dr-lex.be/info-stuff/volumecontrols.html>
    ///
    /// **note: it clamps values outside this range.**
    Normalized(f32),
}

impl Factor {
    pub(crate) fn as_linear(&self) -> f32 {
        match self {
            Factor::Linear(v) => *v,
            Factor::Decibel(db) => db_to_linear(*db),
            Factor::Normalized(normalized) => normalized_to_linear(*normalized),
        }
    }
}

impl Default for Factor {
    /// Keep the volume unchanged
    fn default() -> Self {
        Self::Linear(1.0)
    }
}

pure_effect! {
    supports_dynamic_source
    /// An effect that changes how loud the sound is.
    struct Amplify {
        factor: f32,
    }

    fn next(&mut self) -> Option<Sample> {
        self.inner.next().map(|value| value * self.factor)
    }

    fn new(source: S, factor: Factor) -> Amplify<Self> {
        Self {
            inner: source,
            factor: factor.as_linear(),
        }
    }

    /// Change the current amplification factor. Note this takes immediate
    /// effect without any smoothing. This can sound jarring.
    pub fn set_factor(&mut self, factor: Factor) {
        self.factor = factor.as_linear()
    }
}
