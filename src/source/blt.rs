use crate::common::{ChannelCount, Float, SampleRate};
use crate::math::PI;
use crate::{Sample, Source};
use std::time::Duration;

// Implemented following https://webaudio.github.io/Audio-EQ-Cookbook/audio-eq-cookbook.html
use super::{detect_span_boundary, reset_seek_span_tracking, SeekError};

/// Builds a `BltFilter` object with a low-pass filter.
pub fn low_pass<I>(input: I, freq: u32) -> BltFilter<I>
where
    I: Source<Item = Sample>,
{
    low_pass_with_q(input, freq, 0.5)
}

/// Builds a `BltFilter` object with a high-pass filter.
pub fn high_pass<I>(input: I, freq: u32) -> BltFilter<I>
where
    I: Source<Item = Sample>,
{
    high_pass_with_q(input, freq, 0.5)
}

/// Same as low_pass but allows the q value (bandwidth) to be changed
pub fn low_pass_with_q<I>(input: I, freq: u32, q: Float) -> BltFilter<I>
where
    I: Source<Item = Sample>,
{
    blt_filter(input, BltFormula::LowPass { freq, q })
}

/// Same as high_pass but allows the q value (bandwidth) to be changed
pub fn high_pass_with_q<I>(input: I, freq: u32, q: Float) -> BltFilter<I>
where
    I: Source<Item = Sample>,
{
    blt_filter(input, BltFormula::HighPass { freq, q })
}

/// Common constructor for BLT filters
fn blt_filter<I>(input: I, formula: BltFormula) -> BltFilter<I>
where
    I: Source<Item = Sample>,
{
    let sample_rate = input.sample_rate();
    let channels = input.channels();

    BltFilter {
        inner: Some(BltInner::new(input, formula, channels)),
        last_sample_rate: sample_rate,
        last_channels: channels,
        samples_counted: 0,
        cached_span_len: None,
    }
}

/// This applies an audio filter, it can be a high or low pass filter.
#[derive(Clone, Debug)]
pub struct BltFilter<I> {
    inner: Option<BltInner<I>>,
    last_sample_rate: SampleRate,
    last_channels: ChannelCount,
    samples_counted: usize,
    cached_span_len: Option<usize>,
}

impl<I> BltFilter<I>
where
    I: Source<Item = Sample>,
{
    /// Modifies this filter so that it becomes a low-pass filter.
    pub fn to_low_pass(&mut self, freq: u32) {
        self.to_low_pass_with_q(freq, 0.5);
    }

    /// Modifies this filter so that it becomes a high-pass filter
    pub fn to_high_pass(&mut self, freq: u32) {
        self.to_high_pass_with_q(freq, 0.5);
    }

    /// Same as to_low_pass but allows the q value (bandwidth) to be changed
    pub fn to_low_pass_with_q(&mut self, freq: u32, q: Float) {
        self.inner
            .as_mut()
            .unwrap()
            .set_formula(BltFormula::LowPass { freq, q });
    }

    /// Same as to_high_pass but allows the q value (bandwidth) to be changed
    pub fn to_high_pass_with_q(&mut self, freq: u32, q: Float) {
        self.inner
            .as_mut()
            .unwrap()
            .set_formula(BltFormula::HighPass { freq, q });
    }

    /// Returns a reference to the inner source.
    #[inline]
    pub fn inner(&self) -> &I {
        self.inner.as_ref().unwrap().inner()
    }

    /// Returns a mutable reference to the inner source.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        self.inner.as_mut().unwrap().inner_mut()
    }

    /// Returns the inner source.
    #[inline]
    pub fn into_inner(self) -> I {
        self.inner.unwrap().into_inner()
    }
}

impl<I> Iterator for BltFilter<I>
where
    I: Source<Item = Sample>,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Sample> {
        let sample = self.inner.as_mut().unwrap().next()?;

        let input_span_len = self.inner.as_ref().unwrap().current_span_len();
        let current_sample_rate = self.inner.as_ref().unwrap().sample_rate();
        let current_channels = self.inner.as_ref().unwrap().channels();

        let (at_boundary, parameters_changed) = detect_span_boundary(
            &mut self.samples_counted,
            &mut self.cached_span_len,
            input_span_len,
            current_sample_rate,
            self.last_sample_rate,
            current_channels,
            self.last_channels,
        );

        if at_boundary && parameters_changed {
            let sample_rate_changed = current_sample_rate != self.last_sample_rate;
            let channels_changed = current_channels != self.last_channels;

            self.last_sample_rate = current_sample_rate;
            self.last_channels = current_channels;

            // If channel count changed, reconstruct with new variant (this also recreates applier)
            // Otherwise, just recreate applier if sample rate changed
            if channels_changed {
                let old_inner = self.inner.take().unwrap();
                let (input, formula) = old_inner.into_parts();
                self.inner = Some(BltInner::new(input, formula, current_channels));
            } else if sample_rate_changed {
                self.inner
                    .as_mut()
                    .unwrap()
                    .recreate_applier(current_sample_rate);
            }
        }

        Some(sample)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.as_ref().unwrap().size_hint()
    }
}

impl<I> ExactSizeIterator for BltFilter<I> where I: Source<Item = Sample> + ExactSizeIterator {}

impl<I> Source for BltFilter<I>
where
    I: Source<Item = Sample>,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.inner.as_ref().unwrap().current_span_len()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.inner.as_ref().unwrap().channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.inner.as_ref().unwrap().sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.inner.as_ref().unwrap().total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.as_mut().unwrap().try_seek(pos)?;

        reset_seek_span_tracking(
            &mut self.samples_counted,
            &mut self.cached_span_len,
            pos,
            self.inner.as_ref().unwrap().current_span_len(),
        );

        Ok(())
    }
}

#[derive(Clone, Debug)]
enum BltInner<I> {
    Mono(BltMono<I>),
    Stereo(BltStereo<I>),
    Multi(BltMulti<I>),
}

impl<I> BltInner<I>
where
    I: Source<Item = Sample>,
{
    fn new(input: I, formula: BltFormula, channels: ChannelCount) -> Self {
        let channels_count = channels.get() as usize;

        let sample_rate = input.sample_rate();
        let applier = formula.to_applier(sample_rate.get());

        match channels_count {
            1 => BltInner::Mono(BltMono {
                input,
                formula,
                applier,
                x_n1: 0.0,
                x_n2: 0.0,
                y_n1: 0.0,
                y_n2: 0.0,
            }),
            2 => BltInner::Stereo(BltStereo {
                input,
                formula,
                applier,
                x_n1: [0.0; 2],
                x_n2: [0.0; 2],
                y_n1: [0.0; 2],
                y_n2: [0.0; 2],
                is_right_channel: false,
            }),
            n => BltInner::Multi(BltMulti {
                input,
                formula,
                applier,
                x_n1: vec![0.0; n].into_boxed_slice(),
                x_n2: vec![0.0; n].into_boxed_slice(),
                y_n1: vec![0.0; n].into_boxed_slice(),
                y_n2: vec![0.0; n].into_boxed_slice(),
                position: 0,
            }),
        }
    }

    fn set_formula(&mut self, formula: BltFormula) {
        let sample_rate = self.inner().sample_rate();
        let applier = formula.to_applier(sample_rate.get());

        match self {
            BltInner::Mono(mono) => {
                mono.formula = formula;
                mono.applier = applier;
            }
            BltInner::Stereo(stereo) => {
                stereo.formula = formula;
                stereo.applier = applier;
            }
            BltInner::Multi(multi) => {
                multi.formula = formula;
                multi.applier = applier;
            }
        }
    }

    fn recreate_applier(&mut self, sample_rate: SampleRate) {
        match self {
            BltInner::Mono(mono) => {
                mono.applier = mono.formula.to_applier(sample_rate.get());
            }
            BltInner::Stereo(stereo) => {
                stereo.applier = stereo.formula.to_applier(sample_rate.get());
            }
            BltInner::Multi(multi) => {
                multi.applier = multi.formula.to_applier(sample_rate.get());
            }
        }
    }

    fn into_parts(self) -> (I, BltFormula) {
        match self {
            BltInner::Mono(mono) => (mono.input, mono.formula),
            BltInner::Stereo(stereo) => (stereo.input, stereo.formula),
            BltInner::Multi(multi) => (multi.input, multi.formula),
        }
    }

    #[inline]
    fn inner(&self) -> &I {
        match self {
            BltInner::Mono(mono) => &mono.input,
            BltInner::Stereo(stereo) => &stereo.input,
            BltInner::Multi(multi) => &multi.input,
        }
    }

    #[inline]
    fn inner_mut(&mut self) -> &mut I {
        match self {
            BltInner::Mono(mono) => &mut mono.input,
            BltInner::Stereo(stereo) => &mut stereo.input,
            BltInner::Multi(multi) => &mut multi.input,
        }
    }

    #[inline]
    fn into_inner(self) -> I {
        match self {
            BltInner::Mono(mono) => mono.input,
            BltInner::Stereo(stereo) => stereo.input,
            BltInner::Multi(multi) => multi.input,
        }
    }
}

impl<I> Iterator for BltInner<I>
where
    I: Source<Item = Sample>,
{
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Sample> {
        match self {
            BltInner::Mono(mono) => mono.process_next(),
            BltInner::Stereo(stereo) => stereo.process_next(),
            BltInner::Multi(multi) => multi.process_next(),
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner().size_hint()
    }
}

impl<I> Source for BltInner<I>
where
    I: Source<Item = Sample>,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        self.inner().current_span_len()
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        self.inner().channels()
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        self.inner().sample_rate()
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.inner().total_duration()
    }

    #[inline]
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        match self {
            BltInner::Mono(mono) => {
                mono.input.try_seek(pos)?;
                mono.x_n1 = 0.0;
                mono.x_n2 = 0.0;
                mono.y_n1 = 0.0;
                mono.y_n2 = 0.0;
            }
            BltInner::Stereo(stereo) => {
                stereo.input.try_seek(pos)?;
                stereo.x_n1 = [0.0; 2];
                stereo.x_n2 = [0.0; 2];
                stereo.y_n1 = [0.0; 2];
                stereo.y_n2 = [0.0; 2];
                stereo.is_right_channel = false;
            }
            BltInner::Multi(multi) => {
                multi.input.try_seek(pos)?;
                multi.x_n1.fill(0.0);
                multi.x_n2.fill(0.0);
                multi.y_n1.fill(0.0);
                multi.y_n2.fill(0.0);
                multi.position = 0;
            }
        }
        Ok(())
    }
}

/// Mono channel BLT filter optimized for single-channel processing.
#[derive(Clone, Debug)]
struct BltMono<I> {
    input: I,
    formula: BltFormula,
    applier: BltApplier,
    x_n1: Float,
    x_n2: Float,
    y_n1: Float,
    y_n2: Float,
}

impl<I> BltMono<I>
where
    I: Source<Item = Sample>,
{
    #[inline]
    fn process_next(&mut self) -> Option<Sample> {
        let sample = self.input.next()?;

        let result = self
            .applier
            .apply(sample, self.x_n1, self.x_n2, self.y_n1, self.y_n2);

        self.y_n2 = self.y_n1;
        self.x_n2 = self.x_n1;
        self.y_n1 = result;
        self.x_n1 = sample;

        Some(result)
    }
}

/// Stereo channel BLT filter with optimized two-channel processing.
#[derive(Clone, Debug)]
struct BltStereo<I> {
    input: I,
    formula: BltFormula,
    applier: BltApplier,
    x_n1: [Float; 2],
    x_n2: [Float; 2],
    y_n1: [Float; 2],
    y_n2: [Float; 2],
    is_right_channel: bool,
}

impl<I> BltStereo<I>
where
    I: Source<Item = Sample>,
{
    #[inline]
    fn process_next(&mut self) -> Option<Sample> {
        let sample = self.input.next()?;

        let channel = self.is_right_channel as usize;
        self.is_right_channel = !self.is_right_channel;

        let result = self.applier.apply(
            sample,
            self.x_n1[channel],
            self.x_n2[channel],
            self.y_n1[channel],
            self.y_n2[channel],
        );

        self.y_n2[channel] = self.y_n1[channel];
        self.x_n2[channel] = self.x_n1[channel];
        self.y_n1[channel] = result;
        self.x_n1[channel] = sample;

        Some(result)
    }
}

/// Generic multi-channel BLT filter for surround sound or other configurations.
#[derive(Clone, Debug)]
struct BltMulti<I> {
    input: I,
    formula: BltFormula,
    applier: BltApplier,
    x_n1: Box<[Float]>,
    x_n2: Box<[Float]>,
    y_n1: Box<[Float]>,
    y_n2: Box<[Float]>,
    position: usize,
}

impl<I> BltMulti<I>
where
    I: Source<Item = Sample>,
{
    #[inline]
    fn process_next(&mut self) -> Option<Sample> {
        let sample = self.input.next()?;

        let channel = self.position;
        self.position = (self.position + 1) % self.x_n1.len();

        let result = self.applier.apply(
            sample,
            self.x_n1[channel],
            self.x_n2[channel],
            self.y_n1[channel],
            self.y_n2[channel],
        );

        self.y_n2[channel] = self.y_n1[channel];
        self.x_n2[channel] = self.x_n1[channel];
        self.y_n1[channel] = result;
        self.x_n1[channel] = sample;

        Some(result)
    }
}

#[derive(Clone, Debug)]
enum BltFormula {
    LowPass { freq: u32, q: Float },
    HighPass { freq: u32, q: Float },
}

impl BltFormula {
    fn to_applier(&self, sampling_frequency: u32) -> BltApplier {
        match *self {
            BltFormula::LowPass { freq, q } => {
                let w0 = 2.0 * PI * freq as Float / sampling_frequency as Float;

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
            BltFormula::HighPass { freq, q } => {
                let w0 = 2.0 * PI * freq as Float / sampling_frequency as Float;
                let cos_w0 = w0.cos();
                let alpha = w0.sin() / (2.0 * q);

                let b0 = (1.0 + cos_w0) / 2.0;
                let b1 = -1.0 - cos_w0;
                let b2 = b0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_w0;
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
    b0: Float,
    b1: Float,
    b2: Float,
    a1: Float,
    a2: Float,
}

impl BltApplier {
    #[inline]
    fn apply(&self, x_n: Float, x_n1: Float, x_n2: Float, y_n1: Float, y_n2: Float) -> Float {
        self.b0 * x_n + self.b1 * x_n1 + self.b2 * x_n2 - self.a1 * y_n1 - self.a2 * y_n2
    }
}
