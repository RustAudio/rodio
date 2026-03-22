//! Span boundary detection for sources with changing parameters.
//!
//! [`Source::current_span_len`] divides a source's sample stream into *spans*:
//! contiguous segments with a stable sample rate and channel count. Sources that
//! depend on stable parameters should embed a [`SpanTracker`] and call
//! [`SpanTracker::advance`] on every sample to know when the parameters may have
//! changed.
//!
//! ## Detection modes
//!
//! [`SpanTracker`] operates in one of two modes, controlled by `cached_span_len`:
//!
//! - **Span-counting mode** (`cached_span_len` is `Some`): the span length is known,
//!   so [`SpanTracker::advance`] counts samples and fires precisely at the boundary.
//!   This is the normal and most performant operating mode at the start of a source
//!   or after seeking to the beginning.
//!
//! - **Seek mode** (`cached_span_len` is `None`): used after [`Source::try_seek`]
//!   lands at an arbitrary position mid-span. The span length at that point is
//!   unknown, so [`SpanTracker::advance`] inspects the source's parameters on every
//!   sample until it detects a change, signalling a boundary. Once found, the tracker
//!   switches back to span-counting mode for the new span.
//!
//! Call [`SpanTracker::seek`] from [`Source::try_seek`] implementations to enter seek
//! mode (or return to span-counting mode when seeking to the start).

use std::time::Duration;

use crate::common::{ChannelCount, SampleRate};
use crate::Source;

/// Per-source state for span boundary detection.
#[derive(Clone, Debug)]
pub struct SpanTracker {
    /// Number of samples emitted since the last detected span boundary.
    pub samples_counted: usize,
    /// In span-counting mode: the expected length of the current span.
    /// `None` in seek mode: boundaries are detected by parameter changes instead.
    pub cached_span_len: Option<usize>,
    /// Sample rate at the start of the current span.
    pub last_sample_rate: SampleRate,
    /// Channel count at the start of the current span.
    pub last_channels: ChannelCount,
}

/// Return value of [`SpanTracker::advance`].
pub struct SpanDetection {
    /// `true` when the just-consumed sample is the first sample of a new span.
    pub at_span_boundary: bool,
    /// `true` when the source's sample rate or channel count changed at this boundary.
    pub parameters_changed: bool,
}

impl SpanTracker {
    pub fn new(sample_rate: SampleRate, channels: ChannelCount) -> Self {
        SpanTracker {
            samples_counted: 0,
            cached_span_len: None,
            last_sample_rate: sample_rate,
            last_channels: channels,
        }
    }

    /// Advances the tracker by one sample and reports whether a span boundary was crossed.
    #[inline]
    pub fn advance(
        &mut self,
        input_span_len: Option<usize>,
        current_sample_rate: SampleRate,
        current_channels: ChannelCount,
    ) -> SpanDetection {
        self.samples_counted = self.samples_counted.saturating_add(1);

        // If input reports no span length, parameters are stable by contract.
        let mut parameters_changed = false;
        let at_span_boundary = input_span_len.is_some_and(|_| {
            let known_boundary = self
                .cached_span_len
                .map(|cached_len| self.samples_counted >= cached_len);

            // In span-counting mode, parameters can only change at a boundary.
            // In seek mode, we check every sample for a parameter change.
            if known_boundary.is_none_or(|at_boundary| at_boundary) {
                parameters_changed = current_channels != self.last_channels
                    || current_sample_rate != self.last_sample_rate;
            }

            known_boundary.unwrap_or(parameters_changed)
        });

        if at_span_boundary {
            self.samples_counted = 0;
            self.cached_span_len = input_span_len;
            if parameters_changed {
                self.last_sample_rate = current_sample_rate;
                self.last_channels = current_channels;
            }
        }

        SpanDetection {
            at_span_boundary,
            parameters_changed,
        }
    }

    /// Updates tracking state after the underlying source has been seeked to `pos`.
    #[inline]
    pub fn seek<I: Source>(&mut self, pos: Duration, source: &I) {
        self.samples_counted = 0;
        self.last_sample_rate = source.sample_rate();
        self.last_channels = source.channels();

        // Seeking to `Duration::ZERO` enters span-counting mode because the span length
        // is known from the start. Any other position enters seek mode because the
        // tracker's position within the current span is unknown.
        if pos == Duration::ZERO {
            self.cached_span_len = source.current_span_len();
        } else {
            self.cached_span_len = None;
        }
    }
}
