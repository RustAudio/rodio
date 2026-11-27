//! Rubato resampler wrapper and implementations.

use std::time::Duration;

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

    /// Whether the input still has the format this was built for.
    #[inline]
    pub fn built_for_input(&self) -> bool {
        let (channels, source_rate) = match self {
            ResampleInner::Passthrough {
                channels,
                source_rate,
                ..
            } => (*channels, *source_rate),
            ResampleInner::Poly(resampler) | ResampleInner::Sinc(resampler) => {
                (resampler.output.channels, resampler.output.source_rate)
            }
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => {
                (resampler.output.channels, resampler.output.source_rate)
            }
        };
        self.input().channels() == channels && self.input().sample_rate() == source_rate
    }

    /// The format of the samples `next` yields: what a resampler was
    /// built for while it still holds samples of it, then the input's.
    #[inline]
    pub fn next_format(&self) -> (ChannelCount, SampleRate) {
        match self {
            ResampleInner::Passthrough { source, .. } => (source.channels(), source.sample_rate()),
            ResampleInner::Poly(resampler) | ResampleInner::Sinc(resampler) => {
                resampler.next_format()
            }
            #[cfg(feature = "rubato-fft")]
            ResampleInner::Fft(resampler) => resampler.next_format(),
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

    pub output_delay_remaining: OutFrameCount,

    /// How much of the span being read is left. Tracked because
    /// `Source::current_span_len` reports the span's total length,
    /// not its remainder. `None` after a seek past the start, which
    /// may land partway through a span: until the format changes, it's
    /// then checked before every sample.
    span_remaining: Option<InSamples>,

    /// Output frames still to come from what rubato has been given.
    pub frames_being_resampled: OutFrameCount,
    target_rate: SampleRate,
    /// Input frames given to rubato and frames taken out, since it was
    /// built or reset. Its output is counted from these totals: rounding
    /// each chunk on its own would count up to a frame too many per
    /// chunk, which came out as silence before a format change.
    frames_in_total: u64,
    frames_out_total: u64,
    /// The last frame read, which pads the chunk that ends at a format
    /// change: rubato would pad it with silence, and the last frames
    /// would fall towards it.
    last_frame: Box<[Sample]>,
    /// Copies of the first frame still to feed rubato: the lead-in ahead
    /// of it, then the frame itself. Its history starts as silence, and
    /// the first frames out, which land just before the input's start,
    /// would be interpolated partly from it: a soft start, and a dip
    /// where a resampler is built mid-stream or after a seek.
    lead_in_copies: usize,
    /// Whether the first frame has been read into `last_frame` for the
    /// lead-in.
    lead_in_read: bool,
}

impl<I: Source, R: rubato::Resampler<Sample>> RubatoResample<I, R> {
    /// Frames of lead-in: as far back as the filter reaches from the
    /// first frame out, which can land up to two frames before the
    /// input's start, with a frame to spare. Rounded up to a whole
    /// number of output frames where that's not many more, so skipping
    /// them keeps the output's timing exactly.
    fn lead_in_frames(resampler: &R, source_rate: SampleRate, target_rate: SampleRate) -> usize {
        const MAX_PERIOD: u32 = 1024;
        let (source, target) = (source_rate.get(), target_rate.get());
        let reach = resampler.output_delay() as u64 + 3;
        let frames = (reach * u64::from(source)).div_ceil(u64::from(target)) as usize + 1;
        let period = source / crate::math::gcd(source, target);
        if period <= MAX_PERIOD {
            frames.next_multiple_of(period as usize)
        } else {
            frames
        }
    }

    /// Calculate the number of output frames to skip for delay compensation.
    pub fn output_delay(
        resampler: &R,
        lead_in: usize,
        source_rate: SampleRate,
        target_rate: SampleRate,
    ) -> OutFrameCount {
        // Skip delay-1 frames to align the first output frame with input position 0.
        let delay_frames = resampler.output_delay();
        let delay_frames = delay_frames.saturating_sub(1);
        // And the frames out for the lead-in, to the nearest: the output
        // moves by less than half a frame, if at all.
        let (target, source) = (u64::from(target_rate.get()), u64::from(source_rate.get()));
        let lead_in = (2 * lead_in as u64 * target + source) / (2 * source);
        OutFrameCount(delay_frames + lead_in as usize)
    }

    /// The copies of the first frame to feed rubato, lead-in and frame
    /// itself, and the output delay of a new or reset resampler.
    fn start_state(
        resampler: &R,
        source_rate: SampleRate,
        target_rate: SampleRate,
    ) -> (usize, OutFrameCount) {
        let lead_in = Self::lead_in_frames(resampler, source_rate, target_rate);
        let delay = Self::output_delay(resampler, lead_in, source_rate, target_rate);
        (lead_in + 1, delay)
    }

    pub fn span_length(&self) -> Option<usize> {
        if !self.output.is_empty() {
            // rest of the output buffer is rest of span
            Some(self.output.current_span_len())
        } else if self.input.is_exhausted() && self.resampler_empty() {
            Some(0)
        } else {
            // True span length unknown, return the smallest possible span since that is
            // always correct
            Some(self.next_format().0.get() as usize)
        }
    }

    /// Whether the output buffer has unconsumed samples.
    pub fn output_has_samples(&self) -> bool {
        !self.output.is_empty()
    }

    /// Resampled samples buffered, not yet yielded.
    #[inline]
    pub fn buffered_len(&self) -> usize {
        self.output.len().raw()
    }

    /// Whether samples of the format this was built for are still to
    /// come, buffered or inside rubato.
    pub fn holds_samples(&self) -> bool {
        self.output_has_samples() || !self.resampler_empty()
    }

    /// See [`ResampleInner::next_format`].
    pub fn next_format(&self) -> (ChannelCount, SampleRate) {
        if self.holds_samples() {
            (self.output.channels, self.output.source_rate)
        } else {
            (self.input.channels(), self.input.sample_rate())
        }
    }

    /// Bounds on the samples left, as `Iterator::size_hint`. The rate
    /// may change after the span being read, so only input up to there
    /// can be counted, and past it there's no upper bound.
    pub fn size_hint(&self) -> (usize, Option<usize>) {
        let channels = self.output.channels.get() as usize;
        let buffered = self.output.len().raw() + self.frames_being_resampled.raw() * channels;
        // Samples out for `frames` more frames in, counted as rubato is.
        let resampled = |frames: usize| {
            let total = self.frames_in_total + frames as u64;
            let frames = self.frames_out_for(total) - self.frames_out_for(self.frames_in_total);
            buffered + frames as usize * channels
        };

        // Input left in the format being resampled.
        let in_format = if self.input.current_span_len().is_none() {
            usize::MAX
        } else if self.input.channels() != self.output.channels
            || self.input.sample_rate() != self.output.source_rate
        {
            0
        } else {
            match self.span_remaining {
                // Read up to a span boundary: the next span starts here.
                Some(InSamples::ZERO) => self.input.current_span_len().unwrap_or(0),
                Some(left) => left.raw(),
                // Somewhere in a span after a seek: none of it is certain.
                None => 0,
            }
        };

        let (lower, upper) = self.input.size_hint();
        let upper = upper
            .filter(|&upper| upper <= in_format)
            .map(|upper| resampled(upper.div_ceil(channels)));
        (resampled(lower.min(in_format) / channels), upper)
    }

    /// Start over after the input seeked to `position`.
    pub fn reset(&mut self, position: Duration) {
        self.resampler.reset();
        self.output.reset();
        // A seek to the start lands at the start of a span. Elsewhere,
        // how much of the span is left isn't known.
        self.span_remaining = (position == Duration::ZERO).then_some(InSamples::ZERO);
        (self.lead_in_copies, self.output_delay_remaining) =
            Self::start_state(&self.resampler, self.output.source_rate, self.target_rate);
        self.lead_in_read = false;
        self.frames_being_resampled = OutFrameCount::ZERO;
        self.frames_in_total = 0;
        self.frames_out_total = 0;
    }

    /// Output frames for `frames_in` input frames: one for each output
    /// instant before the end of the input.
    fn frames_out_for(&self, frames_in: u64) -> u64 {
        let target = u128::from(self.target_rate.get());
        let source = u128::from(self.output.source_rate.get());
        (u128::from(frames_in) * target).div_ceil(source) as u64
    }

    /// Whether reading stopped at a span in another format, rather than
    /// at the end of the stream.
    fn stopped_at_format_change(&self) -> bool {
        self.input.current_span_len() != Some(0)
            && (self.input.channels() != self.output.channels
                || self.input.sample_rate() != self.output.source_rate)
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
        let (frames_buffered, frames_in) = self.fill_input_buffer(needed_by_resampler);
        if frames_buffered == InFrameCount::ZERO && self.resampler_empty() {
            return None;
        }

        self.frames_in_total += frames_in.raw() as u64;
        self.frames_being_resampled = OutFrameCount(
            (self.frames_out_for(self.frames_in_total) - self.frames_out_total) as usize,
        );

        let mut partial_len =
            (frames_buffered < needed_by_resampler).then_some(frames_buffered.raw());
        if partial_len.is_some() && self.stopped_at_format_change() {
            // Hold the last frame instead, so the frames before the change
            // don't fall towards silence. They're all that's taken out.
            while self.input_buffer.len().frames(self.output.channels) < needed_by_resampler {
                for &sample in &self.last_frame {
                    self.input_buffer.push(sample);
                }
            }
            partial_len = None;
        }

        let indexing = Some(&rubato::Indexing {
            input_offset: 0,
            output_offset: 0,
            partial_len,
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

        let taken = self.output.len().frames(self.output.channels);
        self.frames_being_resampled -= taken;
        self.frames_out_total += taken.raw() as u64;

        Some(())
    }

    /// Reads across spans, since the resampler is built for one format
    /// and only a span that changes it has to end the chunk. A short
    /// read then means end of stream or a format change, which is when
    /// rubato should treat the chunk as partial.
    ///
    /// Returns the frames buffered, lead-in included, and the frames
    /// read from the input.
    fn fill_input_buffer(
        &mut self,
        needed_by_resampler: InFrameCount,
    ) -> (InFrameCount, InFrameCount) {
        let channels = self.output.channels;
        let wanted = needed_by_resampler.samples(channels);
        self.input_buffer.clear();

        let mut read = InSamples::ZERO;
        if self.lead_in_copies > 0 {
            read = self.lead_in_with_first_frame(wanted);
        }
        if self.lead_in_copies > 0 {
            // The lead-in fills this chunk, and goes on into the next.
            return (
                self.input_buffer.len().frames(channels),
                read.frames(channels),
            );
        }
        read += self.read_input(wanted);

        let len = self.input_buffer.len().raw();
        let frame = self.last_frame.len();
        if len >= frame {
            self.last_frame
                .copy_from_slice(&self.input_buffer.samples[len - frame..len]);
        }
        (
            self.input_buffer.len().frames(channels),
            read.frames(channels),
        )
    }

    /// Reads the first frame and buffers the lead-in ahead of it, as
    /// much of both as fits before `wanted`. Returns the samples read.
    fn lead_in_with_first_frame(&mut self, wanted: InSamples) -> InSamples {
        let mut read = InSamples::ZERO;
        if !self.lead_in_read {
            read = self.read_input(InSamples(self.last_frame.len()));
            if read.raw() < self.last_frame.len() {
                // No whole frame to lead in with: leave what was read
                // as it is.
                self.lead_in_copies = 0;
                return read;
            }
            self.last_frame
                .copy_from_slice(&self.input_buffer.samples[..read.raw()]);
            self.input_buffer.clear();
            self.lead_in_read = true;
        }

        // The first frame itself is the last copy, so it stays pending
        // until every copy ahead of it is in, even when they fill a
        // chunk exactly.
        let room = (wanted - self.input_buffer.len()).raw() / self.last_frame.len();
        let copies = self.lead_in_copies.min(room);
        for _ in 0..copies {
            for &sample in &self.last_frame {
                self.input_buffer.push(sample);
            }
        }
        self.lead_in_copies -= copies;
        if self.lead_in_copies == 0 {
            self.lead_in_read = false;
        }
        read
    }

    /// Reads up to `wanted` samples in all into the input buffer, as far
    /// as the format this was built for goes. Returns the samples read.
    fn read_input(&mut self, wanted: InSamples) -> InSamples {
        let start = self.input_buffer.len();
        while self.input_buffer.len() < wanted {
            let more = match self.span_remaining {
                Some(InSamples::ZERO) => self.start_of_span(),
                Some(_) => true,
                None if self.input.current_span_len().is_none() => self.start_of_span(),
                None => self.input_in_format(),
            };
            if !more {
                break;
            }
            let Some(sample) = self.input.next() else {
                break;
            };
            self.input_buffer.push(sample);
            if let Some(left) = &mut self.span_remaining {
                *left -= InSamples(1);
            }
        }
        self.input_buffer.len() - start
    }

    /// Whether reading may continue into the next span, recording its
    /// length. `false` once the stream ends or the format changes.
    fn start_of_span(&mut self) -> bool {
        let Some(span_len) = self.input.current_span_len() else {
            // Without spans the format never changes.
            self.span_remaining = Some(InSamples::MAX);
            return true;
        };
        if !self.input_in_format() {
            return false;
        }
        self.span_remaining = Some(InSamples(span_len));
        true
    }

    /// Whether the input has samples left in the format being resampled.
    fn input_in_format(&self) -> bool {
        self.input.current_span_len() != Some(0)
            && self.input.channels() == self.output.channels
            && self.input.sample_rate() == self.output.source_rate
    }

    /// A resampler is built at the start of a span, so all of it is left.
    fn first_span(input: &I) -> Option<InSamples> {
        Some(input.current_span_len().map_or(InSamples::MAX, InSamples))
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

        let (lead_in_copies, initial_output_delay) =
            RubatoResample::<I, rubato::Async<Sample>>::start_state(
                &resampler,
                source_rate,
                target_rate,
            );

        let span_remaining = Self::first_span(&input);
        Ok(Self {
            input,
            resampler,
            input_buffer: Input::new(input_buf_size.samples(channels)),
            output: Output::new(source_rate, channels, output_buf_size),
            span_remaining,
            output_delay_remaining: initial_output_delay,
            frames_being_resampled: OutFrameCount::ZERO,
            target_rate,
            frames_in_total: 0,
            frames_out_total: 0,
            last_frame: vec![0.0; channels.get().into()].into_boxed_slice(),
            lead_in_copies,
            lead_in_read: false,
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

        let (lead_in_copies, initial_output_delay) =
            RubatoResample::<I, rubato::Async<Sample>>::start_state(
                &resampler,
                source_rate,
                target_rate,
            );

        let span_remaining = Self::first_span(&input);
        Ok(Self {
            input,
            resampler,
            input_buffer: Input::new(input_buf_size.samples(channels)),
            output: Output::new(source_rate, channels, output_buf_size),
            span_remaining,
            output_delay_remaining: initial_output_delay,
            frames_being_resampled: OutFrameCount::ZERO,
            target_rate,
            frames_in_total: 0,
            frames_out_total: 0,
            last_frame: vec![0.0; channels.get().into()].into_boxed_slice(),
            lead_in_copies,
            lead_in_read: false,
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
        let (lead_in_copies, output_delay_remaining) =
            RubatoFftResample::<I>::start_state(&resampler, source_rate, target_rate);

        let span_remaining = Self::first_span(&input);
        Ok(Self {
            input,
            resampler,
            input_buffer: Input::new(input_buf_size.samples(channels)),
            output: Output::new(source_rate, channels, output_buf_size),
            span_remaining,
            output_delay_remaining,
            frames_being_resampled: OutFrameCount::ZERO,
            target_rate,
            frames_in_total: 0,
            frames_out_total: 0,
            last_frame: vec![0.0; channels.get().into()].into_boxed_slice(),
            lead_in_copies,
            lead_in_read: false,
        })
    }
}
