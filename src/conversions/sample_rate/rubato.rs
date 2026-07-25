//! Rubato resampler wrapper and implementations.

use rubato::{audioadapter_buffers::direct::InterleavedSlice, Resampler};

use super::{InFrameCount, InSamples, OutFrameCount};
use crate::common::{ChannelCount, SampleRate};
use crate::conversions::sample_rate::buffer::{Input, Output};
use crate::{Float, Sample, Source};

use super::builder::{Interpolation, Poly, WindowFunction};

#[derive(thiserror::Error, Debug)]
#[error("Failed to create resampler")]
pub struct ResamplerCreationError(#[from] rubato::ResamplerConstructionError);

/// Type alias for Async (polynomial/sinc) resampler.
pub type RubatoAsyncResample<I> = RubatoResample<I, rubato::Async<Sample>>;

/// Type alias for FFT resampler (synchronous, fixed-ratio).
#[cfg(feature = "rubato-fft")]
pub type RubatoFftResample<I> = RubatoResample<I, rubato::Fft<Sample>>;

/// The inner resampler implementation chosen based on configuration and sample rates.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ResampleInner<I: Source> {
    /// Passthrough when source rate is equal to the target rate
    Passthrough {
        source: I,
        input_span_pos: InSamples,
        channels: ChannelCount,
        source_rate: SampleRate,
    },

    /// Polynomial resampling (fast, no anti-aliasing)
    Poly(RubatoAsyncResample<I>),

    /// Sinc resampling (with anti-aliasing)
    Sinc(RubatoAsyncResample<I>),

    /// FFT resampling for fixed ratios (synchronous resampling)
    #[cfg(feature = "rubato-fft")]
    #[cfg_attr(docsrs, doc(cfg(feature = "rubato-fft")))]
    Fft(RubatoFftResample<I>),
}

impl<I: Source> ResampleInner<I> {
    /// Get a reference to the inner input source
    #[inline]
    pub fn input(&self) -> &I {
        match self {
            ResampleInner::Passthrough { source, .. } => source,
            ResampleInner::Poly(resampler) => &resampler.input,
            ResampleInner::Sinc(resampler) => &resampler.input,
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => &resampler.input,
        }
    }

    /// Extract the inner input source, consuming the resampler
    #[inline]
    pub fn into_inner(self) -> I {
        match self {
            ResampleInner::Passthrough { source, .. } => source,
            ResampleInner::Poly(resampler) => resampler.input,
            ResampleInner::Sinc(resampler) => resampler.input,
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => resampler.input,
        }
    }
}

/// Generic wrapper around Rubato resamplers for sample-by-sample iteration.
#[derive(Debug)]
pub struct RubatoResample<I: Source, R: rubato::Resampler<Sample>> {
    pub input: I,
    pub resampler: R,

    pub input_buffer: super::buffer::Input,
    pub(crate) output: super::buffer::Output,

    /// The following are cached at construction for parameter-change detection.
    pub resample_ratio: Float,

    pub output_delay_remaining: OutFrameCount,
    pub pos_in_current_span: InSamples,

    pub frames_being_resampled: OutFrameCount,
}

impl<I: Source, R: rubato::Resampler<Sample>> RubatoResample<I, R> {
    /// Calculate the number of output samples to skip for delay compensation.
    pub fn output_delay(resampler: &R) -> OutFrameCount {
        // Skip delay-1 frames to align the first output frame with input position 0.
        let delay_frames = resampler.output_delay();
        let delay_frames = delay_frames.saturating_sub(1);
        OutFrameCount(delay_frames)
    }

    pub fn span_length(&self) -> Option<usize> {
        if !self.output.is_empty() {
            // rest of the output buffer is rest of span
            Some(self.output.current_span_len())
        } else if self.input.is_exhausted() {
            Some(0)
        } else {
            // True span length unknown, return the smallest possible span since that is
            // always correct
            Some(self.input.channels().get() as usize)
        }
    }

    /// Whether the output buffer has unconsumed samples.
    pub fn output_has_samples(&self) -> bool {
        !self.output.is_empty()
    }

    pub fn reset(&mut self) {
        self.resampler.reset();
        self.output.reset();
        self.pos_in_current_span = InSamples::ZERO;
        self.output_delay_remaining = Self::output_delay(&self.resampler);
    }

    pub fn next_sample(&mut self) -> Option<Sample> {
        loop {
            if let Some(sample) = self.output.next() {
                return Some(sample);
            }

            // We could get so much output delay that multiple chunks are
            // all silence which is why there is a loop here.
            std::hint::cold_path();
            self.resample_chunk()?;
        }
    }

    // Extracted such that rustc has an easier time outlining this.
    //
    // This is complicated since we need to handle changing sample rates (spans)
    #[inline(never)]
    fn resample_chunk(&mut self) -> Option<()> {
        let needed_by_resampler = InFrameCount(self.resampler.input_frames_next());
        let frames_in = self.fill_input_buffer(needed_by_resampler);
        if frames_in == InFrameCount::ZERO && self.resampler_empty() {
            return None;
        }

        self.frames_being_resampled += frames_in.resampled_by(self.resample_ratio);
        self.pos_in_current_span += frames_in.samples(self.output.channels);

        let indexing = Some(&rubato::Indexing {
            input_offset: 0,
            output_offset: 0,
            partial_len: (frames_in < needed_by_resampler).then_some(frames_in.raw()),
            active_channels_mask: None,
        });

        let input_adapter = InterleavedSlice::new(
            self.input_buffer.as_slice(),
            self.output.channels.get().into(),
            needed_by_resampler.raw(),
        )
        .expect("We always set up the input_buffer correctly");

        let channels = self.output.channels.get().into();
        let capacity = self.output.capacity().raw();
        let mut output_adapter = InterleavedSlice::new_mut(self.output.reset(), channels, capacity)
            .expect("We always set up the input_buffer correctly");

        let (_, frames_out) = self
            .resampler
            .process_into_buffer(&input_adapter, &mut output_adapter, indexing)
            .map(|(r#in, out)| (InFrameCount(r#in), OutFrameCount(out)))
            .expect("We always set up the input_buffer correctly");

        if self.input.is_exhausted() && frames_out == OutFrameCount::ZERO {
            return None;
        }

        let delay_in_output = self.output_delay_remaining.min(frames_out);
        let resampled_frames = self.frames_being_resampled.min(frames_out);

        self.output_delay_remaining -= delay_in_output;

        self.output.set_start(delay_in_output);
        self.output.set_end(frames_out);
        self.output.set_len(resampled_frames);

        self.frames_being_resampled -= self.output.len().frames(self.output.channels);

        Some(())
    }

    fn fill_input_buffer(&mut self, needed_by_resampler: InFrameCount) -> InFrameCount {
        let wanted = needed_by_resampler.samples(self.output.channels);
        self.input_buffer.clear();

        // Keep pulling across span boundaries while the format is unchanged.
        // A span boundary only matters to the resampler when the sample rate
        // or channel count actually changes; ending a chunk at every boundary
        // would flag it `partial_len`, which rubato zero-pads.
        while self.input_buffer.len() < wanted {
            let span = self.input.current_span_len().map(InSamples);
            let take = (wanted - self.input_buffer.len()).min(span.unwrap_or(InSamples::MAX));
            if take == InSamples::ZERO {
                break;
            }

            let rate_before = self.input.sample_rate();
            let channels_before = self.input.channels();

            let mut taken = 0usize;
            for _ in 0..take.raw() {
                if let Some(sample) = self.input.next() {
                    self.input_buffer.push(sample);
                    taken += 1;
                } else {
                    break;
                }
            }

            if taken == 0
                || self.input.sample_rate() != rate_before
                || self.input.channels() != channels_before
            {
                break;
            }
        }

        self.input_buffer.len().frames(self.output.channels)
    }

    fn resampler_empty(&self) -> bool {
        self.frames_being_resampled == OutFrameCount::ZERO
    }
}

// Async resampler (polynomial and sinc) implementations
impl<I: Source> RubatoAsyncResample<I> {
    pub fn new_poly(
        input: I,
        target_rate: SampleRate,
        chunk_size: usize,
        degree: Poly,
    ) -> Result<Self, ResamplerCreationError> {
        let source_rate = input.sample_rate();
        let channels = input.channels();

        let resample_ratio = target_rate.get() as Float / source_rate.get() as Float;

        let resampler = rubato::Async::new_poly(
            resample_ratio as _,
            1.0,
            degree.into(),
            chunk_size,
            channels.get() as usize,
            rubato::FixedAsync::Output,
        )?;

        let input_buf_size = InFrameCount(resampler.input_frames_max());
        let output_buf_size = OutFrameCount(resampler.output_frames_max());

        let initial_output_delay =
            RubatoResample::<I, rubato::Async<Sample>>::output_delay(&resampler);

        Ok(Self {
            input,
            resampler,
            input_buffer: Input::new(input_buf_size.samples(channels)),
            output: Output::new(source_rate, channels, output_buf_size),
            pos_in_current_span: InSamples::ZERO,
            output_delay_remaining: initial_output_delay,
            resample_ratio,
            frames_being_resampled: OutFrameCount::ZERO,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_sinc(
        input: I,
        target_rate: SampleRate,
        chunk_size: usize,
        sinc_len: usize,
        f_cutoff: Float,
        oversampling_factor: usize,
        interpolation: Interpolation,
        window: WindowFunction,
    ) -> Result<Self, ResamplerCreationError> {
        let source_rate = input.sample_rate();
        let channels = input.channels();

        let parameters = rubato::SincInterpolationParameters {
            sinc_len,
            f_cutoff: f_cutoff as _,
            oversampling_factor,
            interpolation: interpolation.into(),
            window: window.into(),
        };

        let resample_ratio = target_rate.get() as Float / source_rate.get() as Float;

        let resampler = rubato::Async::new_sinc(
            resample_ratio as _,
            1.0,
            &parameters,
            chunk_size,
            channels.get() as usize,
            rubato::FixedAsync::Output,
        )?;

        let input_buf_size = InFrameCount(resampler.input_frames_max());
        let output_buf_size = OutFrameCount(resampler.output_frames_max());

        let initial_output_delay =
            RubatoResample::<I, rubato::Async<Sample>>::output_delay(&resampler);

        Ok(Self {
            input,
            resampler,
            input_buffer: Input::new(input_buf_size.samples(channels)),
            output: Output::new(source_rate, channels, output_buf_size),
            pos_in_current_span: InSamples::ZERO,
            output_delay_remaining: initial_output_delay,
            resample_ratio,
            frames_being_resampled: OutFrameCount::ZERO,
        })
    }
}

// FFT resampler implementation
#[cfg(feature = "rubato-fft")]
impl<I: Source> RubatoFftResample<I> {
    /// Create a new FFT resampler for fixed-ratio sample rate conversion.
    ///
    /// The FFT resampler requires that:
    /// - Input chunk size must be a multiple of the GCD-reduced denominator
    /// - Output chunk size must be a multiple of the GCD-reduced numerator
    pub fn new(
        input: I,
        target_rate: SampleRate,
        chunk_size: usize,
        sub_chunks: usize,
    ) -> Result<Self, ResamplerCreationError> {
        let source_rate = input.sample_rate();
        let channels = input.channels();

        // Determine input chunk size - must be multiple of the GCD-reduced denominator
        let g = crate::math::gcd(target_rate.get(), source_rate.get());
        let den = (source_rate.get() / g) as usize;
        let input_chunk_size = ((chunk_size / den) + 1) * den;

        let resampler = rubato::Fft::new(
            source_rate.get() as usize,
            target_rate.get() as usize,
            input_chunk_size,
            sub_chunks,
            channels.get() as usize,
            rubato::FixedSync::Output,
        )?;

        let input_buf_size = InFrameCount(resampler.input_frames_max());
        let output_buf_size = OutFrameCount(resampler.output_frames_max());
        let resample_ratio = target_rate.get() as Float / source_rate.get() as Float;

        let output_delay_remaining = RubatoFftResample::<I>::output_delay(&resampler);

        Ok(Self {
            input,
            resampler,
            input_buffer: Input::new(input_buf_size.samples(channels)),
            output: Output::new(source_rate, channels, output_buf_size),
            pos_in_current_span: InSamples::ZERO,
            output_delay_remaining,
            resample_ratio,
            frames_being_resampled: OutFrameCount::ZERO,
        })
    }
}
