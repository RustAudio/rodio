/// A iterator that stores extracted data in memory while allowing
/// concurrent reading in real time.
use std::mem;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::SeekError;
use crate::common::{ChannelCount, SampleRate};
use crate::math::{ch, PrevMultipleOf};
use crate::Source;

/// Iterator that at the same time extracts data from the iterator and
/// stores it in a buffer.
pub struct Buffered<I>
where
    I: Source,
{
    /// Immutable reference to the next span of data. Cannot be `Span::Input`.
    current_span: Arc<Span<I>>,

    parameters_changed: bool,

    /// The position in number of samples of this iterator inside `current_span`.
    position_in_span: usize,

    /// Obtained once at creation and never modified again.
    total_duration: Option<Duration>,
}

impl<I: Source> Buffered<I> {
    pub(crate) fn new(input: I) -> Buffered<I> {
        let total_duration = input.total_duration();
        let first_span = extract(input);

        Buffered {
            current_span: first_span,
            position_in_span: 0,
            total_duration,
            parameters_changed: false,
        }
    }

    /// Advances to the next span.
    fn next_span(&mut self) {
        let next_span = {
            let mut next_span_ptr = match &*self.current_span {
                Span::Data(SpanData { next, .. }) => next.lock().unwrap(),
                _ => unreachable!(),
            };

            let next_span = match &**next_span_ptr {
                Span::Data(_) => next_span_ptr.clone(),
                Span::End => next_span_ptr.clone(),
                Span::Input(input) => {
                    let input = input.lock().unwrap().take().unwrap();
                    extract(input)
                }
            };

            *next_span_ptr = next_span.clone();
            next_span
        };

        self.current_span = next_span;
        self.position_in_span = 0;
    }
}

impl<I> Iterator for Buffered<I>
where
    I: Source,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        let current_sample;
        let advance_span;

        match &*self.current_span {
            Span::Data(SpanData { data, .. }) => {
                current_sample = Some(data[self.position_in_span]);
                self.position_in_span += 1;
                advance_span = self.position_in_span >= data.len();
            }

            Span::End => {
                current_sample = None;
                advance_span = false;
            }

            Span::Input(_) => unreachable!(),
        };

        if advance_span {
            self.parameters_changed = true;
            self.next_span();
        } else {
            self.parameters_changed = false;
        }

        current_sample
    }
}

impl<I> Source for Buffered<I>
where
    I: Source,
{
    #[inline]
    fn parameters_changed(&self) -> bool {
        self.parameters_changed
    }

    #[inline]
    fn channels(&self) -> ChannelCount {
        match *self.current_span {
            Span::Data(SpanData { channels, .. }) => channels,
            Span::End => ch!(1),
            Span::Input(_) => unreachable!(),
        }
    }

    #[inline]
    fn sample_rate(&self) -> SampleRate {
        match *self.current_span {
            Span::Data(SpanData { rate, .. }) => rate,
            Span::End => 1,
            Span::Input(_) => unreachable!(),
        }
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }

    /// Can not support seek, in the end state we lose the underlying source
    /// which makes seeking back impossible.
    #[inline]
    fn try_seek(&mut self, _: Duration) -> Result<(), SeekError> {
        Err(SeekError::NotSupported {
            underlying_source: std::any::type_name::<Self>(),
        })
    }
}

impl<I> Clone for Buffered<I>
where
    I: Source,
{
    #[inline]
    fn clone(&self) -> Buffered<I> {
        Buffered {
            current_span: self.current_span.clone(),
            position_in_span: self.position_in_span,
            total_duration: self.total_duration,
            parameters_changed: self.parameters_changed,
        }
    }
}

enum Span<I>
where
    I: Source,
{
    /// Data that has already been extracted from the iterator.
    /// Also contains a pointer to the next span.
    Data(SpanData<I>),

    /// No more data.
    End,

    /// Unextracted data. The `Option` should never be `None` and is only here for easier data
    /// processing.
    Input(Mutex<Option<I>>),
}

impl<I: Source> std::fmt::Debug for Span<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Span::Data(_) => f.write_str("Span::Data"),
            Span::End => f.write_str("Span::End"),
            Span::Input(_) => f.write_str("Span::Input"),
        }
    }
}

struct SpanData<I>
where
    I: Source,
{
    data: Vec<I::Item>,
    channels: ChannelCount,
    rate: SampleRate,
    next: Mutex<Arc<Span<I>>>,
}

impl<I> Drop for SpanData<I>
where
    I: Source,
{
    fn drop(&mut self) {
        // This is necessary to prevent stack overflows deallocating long chains of the mutually
        // recursive `Span` and `SpanData` types. This iteratively traverses as much of the
        // chain as needs to be deallocated, and repeatedly "pops" the head off the list. This
        // solves the problem, as when the time comes to actually deallocate the `SpanData`,
        // the `next` field will contain a `Span::End`, or an `Arc` with additional references,
        // so the depth of recursive drops will be bounded.
        while let Ok(arc_next) = self.next.get_mut() {
            if let Some(next_ref) = Arc::get_mut(arc_next) {
                // This allows us to own the next Span.
                let next = mem::replace(next_ref, Span::End);
                if let Span::Data(next_data) = next {
                    // Swap the current SpanData with the next one, allowing the current one
                    // to go out of scope.
                    *self = next_data;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
}

/// Builds a span from the input iterator.
fn extract<I>(mut input: I) -> Arc<Span<I>>
where
    I: Source,
{
    let channels = input.channels();
    let rate = input.sample_rate();

    let mut data = Vec::new();
    loop {
        let Some(sample) = input.next() else {
            break;
        };
        data.push(sample);
        if input.parameters_changed() {
            break;
        }
        if data.len() > 32768.prev_multiple_of(channels.into()) {
            break;
        }
    }

    if data.is_empty() {
        return Arc::new(Span::End);
    }

    Arc::new(Span::Data(SpanData {
        data,
        channels,
        rate,
        next: Mutex::new(Arc::new(Span::Input(Mutex::new(Some(input))))),
    }))
}
