//! Fixed-capacity sample buffer with a read cursor.
//!
use std::fmt::{Debug, Write};

use super::{InSamples, OutFrameCount, OutSamples};
use crate::{ChannelCount, Sample};

pub(crate) struct Input {
    pub samples: Box<[Sample]>,
    pos: InSamples,
}

impl Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Input")
            .field("pos", &self.pos)
            .field("data", &LimitLength(&self.samples[0..self.pos.raw()]))
            .finish()
    }
}

impl Input {
    pub(crate) fn new(capacity: InSamples) -> Self {
        let mut samples = Vec::new();
        samples.reserve_exact(capacity.raw());
        samples.resize(samples.capacity(), 0.0);
        Self {
            samples: samples.into_boxed_slice(),
            pos: InSamples::ZERO,
        }
    }

    pub(crate) fn push(&mut self, sample: Sample) {
        assert!(
            self.pos.raw() < self.samples.len(),
            "pos: {:?}, capacity: {}",
            self.pos,
            self.samples.len()
        );
        self.samples[self.pos.raw()] = sample;
        self.pos += 1;
    }

    pub(crate) fn as_slice(&mut self) -> &[Sample] {
        &self.samples
    }

    pub(crate) fn clear(&mut self) {
        self.pos = InSamples::ZERO;
    }

    pub(crate) fn len(&self) -> InSamples {
        self.pos
    }
}

pub(crate) struct Output {
    start: OutSamples,
    pos: OutSamples,
    end: OutSamples,

    pub samples: Box<[Sample]>,
    pub channels: ChannelCount,
}

impl Debug for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Output")
            .field("start", &self.start)
            .field("pos", &self.pos)
            .field("end", &self.end)
            .field(
                "data",
                &LimitLength(&self.samples[self.pos.raw()..self.end.raw()]),
            )
            .field("channels", &self.channels)
            .finish()
    }
}

struct LimitLength<'a>(&'a [Sample]);

impl Debug for LimitLength<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_empty() {
            return f.write_str("[]");
        } else if self.0.len() < 8 {
            return f.debug_list().entries(self.0).finish();
        }

        f.write_str("[")?;
        for element in self.0.iter().take(3) {
            f.write_str("\n\t")?;
            f.write_fmt(format_args!("{element:?}"))?;
            f.write_char(',')?;
        }

        if let Some(hidden) = self.0.len().checked_sub(6) {
            f.write_str("\n\t.. ")?;
            f.write_fmt(format_args!(" (hiding {hidden} entries)"))?;
        }

        for element in self.0.iter().rev().take(3).rev() {
            f.write_str("\n\t")?;
            f.write_fmt(format_args!("{element:?}"))?;
            f.write_char(',')?;
        }

        f.write_str("\n]")
    }
}

impl Output {
    pub(super) fn new(channels: ChannelCount, capacity: OutFrameCount) -> Self {
        let mut samples = Vec::new();
        samples.reserve_exact(capacity.samples(channels).raw());
        samples.resize(samples.capacity(), 0.0);
        Self {
            start: OutSamples::ZERO,
            pos: OutSamples::ZERO,
            end: OutSamples::ZERO,
            samples: samples.into_boxed_slice(),
            channels,
        }
    }

    pub(super) fn capacity(&self) -> OutFrameCount {
        OutSamples(self.samples.len()).frames(self.channels)
    }

    pub(crate) fn len(&self) -> OutSamples {
        self.end - self.pos
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len().raw() == 0
    }

    pub(super) fn reset(&mut self) -> &mut [Sample] {
        self.pos = OutSamples::ZERO;
        self.start = OutSamples::ZERO;
        self.end = OutSamples::ZERO;
        &mut self.samples
    }

    pub(super) fn set_start(&mut self, start: OutFrameCount) {
        self.start = start.samples(self.channels);
        self.pos = self.start;
    }

    pub(super) fn set_end(&mut self, end: OutFrameCount) {
        self.end = end.samples(self.channels);
    }

    pub(crate) fn set_len(&mut self, len: OutFrameCount) {
        self.end = self.end.min(self.start + len.samples(self.channels));
        self.assert_view_makes_sense();
    }

    pub(super) fn current_span_len(&self) -> usize {
        (self.end - self.start).raw()
    }

    #[track_caller]
    fn assert_view_makes_sense(&self) {
        assert!(self.start.raw() <= self.samples.len());
        assert!(
            self.start <= self.end,
            "start ({:?}) may not be after end ({:?})",
            self.start,
            self.end
        );
        assert!(self.end.raw() <= self.samples.len());
    }
}

impl Iterator for Output {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.end {
            None
        } else {
            let sample = self.samples[self.pos.raw()];
            self.pos += 1usize;
            Some(sample)
        }
    }
}
