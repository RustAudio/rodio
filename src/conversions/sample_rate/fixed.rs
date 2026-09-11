//! Fixed-ratio, polyphase windowed-sinc resampling.
//!
//! This is intentionally an optional backend. It is specialized for f32, fixed rational ratios,
//! and interleaved Sources; Rubato remains the general-purpose implementation and fallback.

use std::f64::consts::PI;

use super::buffer::{Input, Output};
use super::builder::{Sinc, WindowFunction};
use super::{InFrameCount, InSamples, OutFrameCount};
use crate::common::{ChannelCount, SampleRate};
use crate::{Sample, Source};

const MAX_PHASES: u32 = 1_280;

#[derive(Clone, Copy, Debug)]
enum Kernel {
    Scalar,
    Avx2,
}

#[derive(Debug, Clone)]
struct Plan {
    input_rate: u32,
    output_rate: u32,
    taps: usize,
    half: usize,
    coefficients: Vec<f32>,
    advances: Vec<u32>,
    kernel: Kernel,
}

impl Plan {
    fn new(source_rate: SampleRate, target_rate: SampleRate, config: &Sinc) -> Self {
        let input_rate = source_rate.get();
        let output_rate = target_rate.get();
        let gcd = crate::math::gcd(input_rate, output_rate);
        let input_step = input_rate / gcd;
        let output_step = output_rate / gcd;
        assert!(input_step <= MAX_PHASES && output_step <= MAX_PHASES);

        // Rubato rounds its sinc table length to a multiple of eight for its SIMD kernels. Keep
        // the same observable filter length even when a caller supplies an unusual tap count.
        let taps = config.sinc_len.next_multiple_of(8);
        let half = taps / 2;
        let cutoff =
            0.5 * (output_rate as f64 / input_rate as f64).min(1.0) * config.f_cutoff as f64;
        let mut coefficients = vec![0.0; output_step as usize * taps];
        let mut advances = Vec::with_capacity(output_step as usize);

        // A rational ratio visits a finite set of exact fractional positions. That makes the
        // interpolation mode and oversampling factor unnecessary here: store the exact phases
        // once instead of interpolating a larger approximate lookup table at run time.
        for cursor in 0..output_step as usize {
            let position = cursor as u64 * input_step as u64;
            let phase = (position % output_step as u64) as usize;
            advances.push(
                ((position + input_step as u64) / output_step as u64
                    - position / output_step as u64) as u32,
            );
            let fraction = phase as f64 / output_step as f64;
            let row = &mut coefficients[cursor * taps..(cursor + 1) * taps];
            let mut sum = 0.0;
            for (tap, coefficient) in row.iter_mut().enumerate() {
                let distance = tap as f64 - half as f64 - fraction;
                let value =
                    2.0 * cutoff * sinc(distance * 2.0 * cutoff) * window(tap, taps, config.window);
                *coefficient = value as f32;
                sum += value;
            }
            if sum != 0.0 {
                for coefficient in row {
                    *coefficient /= sum as f32;
                }
            }
        }

        Self {
            input_rate,
            output_rate,
            taps,
            half,
            coefficients,
            advances,
            kernel: if avx2_available() {
                Kernel::Avx2
            } else {
                Kernel::Scalar
            },
        }
    }

    #[inline]
    fn row(&self, phase: usize) -> &[f32] {
        let start = phase * self.taps;
        &self.coefficients[start..start + self.taps]
    }
}

/// Streaming adapter around the fixed-ratio core.
#[derive(Debug)]
pub struct FixedResample<I: Source> {
    pub input: I,
    pub input_buffer: Input,
    pub(crate) output: Output,
    plan: Plan,
    core: Core,
    pub resample_ratio: f32,
    pub pos_in_current_span: InSamples,
    pub frames_being_resampled: OutFrameCount,
}

impl<I: Source> FixedResample<I> {
    pub fn new(input: I, target_rate: SampleRate, sinc: &Sinc) -> Self {
        let source_rate = input.sample_rate();
        let channels = input.channels();
        let plan = Plan::new(source_rate, target_rate, sinc);
        let input_frames = sinc.chunk_size.max(1);
        let output_frames =
            ((input_frames as u64 * target_rate.get() as u64 + source_rate.get() as u64 - 1)
                / source_rate.get() as u64) as usize
                + sinc.sinc_len
                + 2;
        Self {
            input,
            input_buffer: Input::new(InFrameCount(input_frames).samples(channels)),
            output: Output::new(source_rate, channels, OutFrameCount(output_frames)),
            core: Core::new(plan.clone(), channels),
            plan,
            resample_ratio: target_rate.get() as f32 / source_rate.get() as f32,
            pos_in_current_span: InSamples::ZERO,
            frames_being_resampled: OutFrameCount::ZERO,
        }
    }

    #[inline]
    pub fn span_length(&self) -> Option<usize> {
        if !self.output.is_empty() {
            Some(self.output.current_span_len())
        } else if self.input.is_exhausted() {
            Some(0)
        } else {
            Some(self.output.channels.get() as usize)
        }
    }

    #[inline]
    pub fn output_has_samples(&self) -> bool {
        !self.output.is_empty()
    }

    #[inline]
    pub fn format_changed(&self) -> bool {
        self.input.sample_rate().get() != self.plan.input_rate
            || self.input.channels() != self.output.channels
    }

    pub fn reset(&mut self) {
        self.core.reset();
        self.input_buffer.clear();
        self.output.reset();
        self.pos_in_current_span = InSamples::ZERO;
        self.frames_being_resampled = OutFrameCount::ZERO;
    }

    pub fn next_sample(&mut self) -> Option<Sample> {
        loop {
            if let Some(sample) = self.output.next() {
                return Some(sample);
            }
            std::hint::cold_path();
            self.resample_chunk()?;
        }
    }

    #[inline(never)]
    fn resample_chunk(&mut self) -> Option<()> {
        let frames_in = self.fill_input_buffer();
        let end_of_input = self.input.is_exhausted() || self.format_changed();
        if frames_in == InFrameCount::ZERO && !end_of_input {
            return None;
        }

        self.pos_in_current_span += frames_in.samples(self.output.channels);
        self.frames_being_resampled += frames_in.resampled_by(self.resample_ratio);
        let frames_out = self.core.process(
            &self.input_buffer.samples[..self.input_buffer.len().raw()],
            &mut self.output.reset()[..],
            end_of_input,
        );
        self.input_buffer.clear();
        if frames_out == 0 {
            // A small processing block may not yet reach the FIR lookahead. Keep pulling input
            // until output becomes available or the source really ends.
            return (!end_of_input).then_some(());
        }
        if end_of_input {
            self.frames_being_resampled = OutFrameCount::ZERO;
        } else {
            self.frames_being_resampled = self
                .frames_being_resampled
                .saturating_sub(OutFrameCount(frames_out));
        }
        self.output.set_start(OutFrameCount::ZERO);
        self.output.set_end(OutFrameCount(frames_out));
        Some(())
    }

    fn fill_input_buffer(&mut self) -> InFrameCount {
        self.input_buffer.clear();
        let capacity = self.input_buffer.samples.len();
        let channels = self.output.channels;
        while self.input_buffer.len().raw() < capacity {
            // Stable-format spans may be joined. Stop before consuming the first sample of a
            // format-changing span so the FIR state is never mixed across channel layouts/rates.
            if self.format_changed() {
                break;
            }
            if let Some(sample) = self.input.next() {
                self.input_buffer.push(sample);
            } else {
                break;
            }
        }
        self.input_buffer.len().frames(channels)
    }
}

#[derive(Debug)]
struct Core {
    plan: Plan,
    channels: ChannelCount,
    input_base: u64,
    total_input: u64,
    next_output: u64,
    next_center: u64,
    phase_cursor: usize,
    finished: bool,
    buffer: Vec<Vec<f32>>,
}

impl Core {
    fn new(plan: Plan, channels: ChannelCount) -> Self {
        Self {
            plan,
            channels,
            input_base: 0,
            total_input: 0,
            next_output: 0,
            next_center: 0,
            phase_cursor: 0,
            finished: false,
            buffer: (0..channels.get()).map(|_| vec![0.0]).collect(),
        }
    }

    fn reset(&mut self) {
        self.input_base = 0;
        self.total_input = 0;
        self.next_output = 0;
        self.next_center = 0;
        self.phase_cursor = 0;
        self.finished = false;
        for channel in &mut self.buffer {
            channel.clear();
            channel.push(0.0);
        }
    }

    fn process(&mut self, input: &[Sample], output: &mut [Sample], finish: bool) -> usize {
        if self.finished {
            return 0;
        }
        let channels = self.channels.get() as usize;
        debug_assert_eq!(input.len() % channels, 0);
        debug_assert_eq!(output.len() % channels, 0);
        for channel in &mut self.buffer {
            channel.pop();
        }
        for frame in input.chunks_exact(channels) {
            for (channel, sample) in frame.iter().enumerate() {
                self.buffer[channel].push(*sample);
            }
        }
        for channel in &mut self.buffer {
            channel.push(0.0);
        }
        self.total_input += (input.len() / channels) as u64;
        self.finished = finish;
        let ready = if finish {
            self.total_input
        } else {
            self.total_input.saturating_sub(self.plan.half as u64)
        };
        let end = ((ready as u128 * self.plan.output_rate as u128 + self.plan.input_rate as u128
            - 1)
            / self.plan.input_rate as u128) as u64;
        let count = end
            .saturating_sub(self.next_output)
            .min((output.len() / channels) as u64) as usize;
        let mut center = self.next_center;
        let mut phase = self.phase_cursor;
        for frame in 0..count {
            let coefficients = self.plan.row(phase);
            if channels == 2 {
                let (left, right) = self.convolve_stereo(center, coefficients);
                output[frame * 2] = left;
                output[frame * 2 + 1] = right;
            } else {
                for channel in 0..channels {
                    output[frame * channels + channel] =
                        self.convolve(channel, center, coefficients);
                }
            }
            center += self.plan.advances[phase] as u64;
            phase += 1;
            if phase == self.plan.advances.len() {
                phase = 0;
            }
        }
        self.next_center = center;
        self.phase_cursor = phase;
        self.next_output += count as u64;
        self.compact();
        count
    }

    #[inline]
    fn convolve(&self, channel: usize, center: u64, coefficients: &[f32]) -> f32 {
        let first = center as i64 - self.plan.half as i64;
        let relative = first - self.input_base as i64;
        let signal = &self.buffer[channel];
        if relative >= 0 && relative as usize + self.plan.taps <= signal.len() {
            return dot(
                &signal[relative as usize..relative as usize + self.plan.taps],
                coefficients,
                self.plan.kernel,
            );
        }
        coefficients
            .iter()
            .enumerate()
            .fold(0.0, |sum, (tap, coefficient)| {
                let index = first + tap as i64;
                if index < self.input_base as i64 {
                    sum
                } else {
                    let local = (index - self.input_base as i64) as usize;
                    signal
                        .get(local)
                        .map_or(sum, |sample| sum + sample * coefficient)
                }
            })
    }

    #[inline]
    fn convolve_stereo(&self, center: u64, coefficients: &[f32]) -> (f32, f32) {
        let first = center as i64 - self.plan.half as i64;
        let relative = first - self.input_base as i64;
        if relative >= 0 && relative as usize + self.plan.taps <= self.buffer[0].len() {
            let start = relative as usize;
            return dot_pair(
                &self.buffer[0][start..start + self.plan.taps],
                &self.buffer[1][start..start + self.plan.taps],
                coefficients,
                self.plan.kernel,
            );
        }
        (
            self.convolve(0, center, coefficients),
            self.convolve(1, center, coefficients),
        )
    }

    fn compact(&mut self) {
        let keep = self.next_center.saturating_sub(self.plan.half as u64);
        if keep <= self.input_base {
            return;
        }
        let discard = ((keep - self.input_base) as usize).min(self.buffer[0].len() - 1);
        if discard == 0 {
            return;
        }
        for channel in &mut self.buffer {
            channel.drain(..discard);
        }
        self.input_base += discard as u64;
    }
}

fn window(index: usize, len: usize, kind: WindowFunction) -> f64 {
    let x = 2.0 * PI * index as f64 / len as f64;
    let base = match kind {
        WindowFunction::Hann | WindowFunction::Hann2 => 0.5 - 0.5 * x.cos(),
        WindowFunction::Blackman | WindowFunction::Blackman2 => {
            0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos()
        }
        WindowFunction::BlackmanHarris | WindowFunction::BlackmanHarris2 => {
            0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos() - 0.01168 * (3.0 * x).cos()
        }
    };
    match kind {
        WindowFunction::Hann2 | WindowFunction::Blackman2 | WindowFunction::BlackmanHarris2 => {
            base * base
        }
        _ => base,
    }
}

#[inline]
fn sinc(x: f64) -> f64 {
    if x.abs() < 1.0e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

fn avx2_available() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        false
    }
}

#[inline(always)]
fn dot(samples: &[f32], coefficients: &[f32], kernel: Kernel) -> f32 {
    match kernel {
        Kernel::Scalar => samples.iter().zip(coefficients).map(|(a, b)| a * b).sum(),
        Kernel::Avx2 => unsafe { dot_avx2(samples, coefficients) },
    }
}

#[inline(always)]
fn dot_pair(left: &[f32], right: &[f32], coefficients: &[f32], kernel: Kernel) -> (f32, f32) {
    match kernel {
        Kernel::Scalar => (
            left.iter().zip(coefficients).map(|(a, b)| a * b).sum(),
            right.iter().zip(coefficients).map(|(a, b)| a * b).sum(),
        ),
        Kernel::Avx2 => unsafe { dot_pair_avx2(left, right, coefficients) },
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn dot_avx2(samples: &[f32], coefficients: &[f32]) -> f32 {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;
    let mut sum = _mm256_setzero_ps();
    let mut i = 0;
    while i + 8 <= samples.len() {
        sum = _mm256_add_ps(
            sum,
            _mm256_mul_ps(
                _mm256_loadu_ps(samples.as_ptr().add(i)),
                _mm256_loadu_ps(coefficients.as_ptr().add(i)),
            ),
        );
        i += 8;
    }
    let mut lanes = [0.0; 8];
    _mm256_storeu_ps(lanes.as_mut_ptr(), sum);
    lanes.iter().sum::<f32>()
        + samples[i..]
            .iter()
            .zip(&coefficients[i..])
            .map(|(a, b)| a * b)
            .sum::<f32>()
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
unsafe fn dot_avx2(samples: &[f32], coefficients: &[f32]) -> f32 {
    samples.iter().zip(coefficients).map(|(a, b)| a * b).sum()
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn dot_pair_avx2(left: &[f32], right: &[f32], coefficients: &[f32]) -> (f32, f32) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;
    let mut sums = [_mm256_setzero_ps(); 2];
    let mut i = 0;
    while i + 8 <= left.len() {
        let c = _mm256_loadu_ps(coefficients.as_ptr().add(i));
        sums[0] = _mm256_add_ps(
            sums[0],
            _mm256_mul_ps(_mm256_loadu_ps(left.as_ptr().add(i)), c),
        );
        sums[1] = _mm256_add_ps(
            sums[1],
            _mm256_mul_ps(_mm256_loadu_ps(right.as_ptr().add(i)), c),
        );
        i += 8;
    }
    let mut lanes = [[0.0; 8]; 2];
    _mm256_storeu_ps(lanes[0].as_mut_ptr(), sums[0]);
    _mm256_storeu_ps(lanes[1].as_mut_ptr(), sums[1]);
    (
        lanes[0].iter().sum::<f32>()
            + left[i..]
                .iter()
                .zip(&coefficients[i..])
                .map(|(a, b)| a * b)
                .sum::<f32>(),
        lanes[1].iter().sum::<f32>()
            + right[i..]
                .iter()
                .zip(&coefficients[i..])
                .map(|(a, b)| a * b)
                .sum::<f32>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_and_avx2_dots_agree() {
        if !avx2_available() {
            return;
        }
        let left: Vec<_> = (0..67).map(|i| (i as f32 * 0.17).sin()).collect();
        let right: Vec<_> = (0..67).map(|i| (i as f32 * 0.23).cos()).collect();
        let coefficients: Vec<_> = (0..67).map(|i| (i as f32 * 0.11).sin()).collect();
        let scalar = dot(&left, &coefficients, Kernel::Scalar);
        let avx = dot(&left, &coefficients, Kernel::Avx2);
        assert!((scalar - avx).abs() < 2.0e-5);
        let scalar_pair = dot_pair(&left, &right, &coefficients, Kernel::Scalar);
        let avx_pair = dot_pair(&left, &right, &coefficients, Kernel::Avx2);
        assert!((scalar_pair.0 - avx_pair.0).abs() < 2.0e-5);
        assert!((scalar_pair.1 - avx_pair.1).abs() < 2.0e-5);
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
unsafe fn dot_pair_avx2(left: &[f32], right: &[f32], coefficients: &[f32]) -> (f32, f32) {
    (
        left.iter().zip(coefficients).map(|(a, b)| a * b).sum(),
        right.iter().zip(coefficients).map(|(a, b)| a * b).sum(),
    )
}
