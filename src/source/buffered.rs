use std::cmp;
use std::mem;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{Sample, Source};

/// Internal function that builds a `Buffered` object.
#[inline]
pub fn buffered<I>(input: I) -> Buffered<I>
where
    I: Source,
    I::Item: Sample,
{
    let total_duration = input.total_duration();
    let first_frame = extract(input);

    Buffered {
        current_frame: first_frame,
        position_in_frame: 0,
        total_duration,
    }
}

/// Iterator that at the same time extracts data from the iterator and stores it in a buffer.
pub struct Buffered<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Immutable reference to the next frame of data. Cannot be `Frame::Input`.
    current_frame: Arc<Frame<I>>,

    /// The position in number of samples of this iterator inside `current_frame`.
    position_in_frame: usize,

    /// Obtained once at creation and never modified again.
    total_duration: Option<Duration>,
}

enum Frame<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Data that has already been extracted from the iterator. Also contains a pointer to the
    /// next frame.
    Data(FrameData<I>),

    /// No more data.
    End,

    /// Unextracted data. The `Option` should never be `None` and is only here for easier data
    /// processing.
    Input(Mutex<Option<I>>),
}

struct FrameData<I>
where
    I: Source,
    I::Item: Sample,
{
    data: Vec<I::Item>,
    channels: u16,
    rate: u32,
    next: Mutex<Arc<Frame<I>>>,
}

impl<I> Drop for FrameData<I>
where
    I: Source,
    I::Item: Sample,
{
    fn drop(&mut self) {
        // This is necessary to prevent stack overflows deallocating long chains of the mutually
        // recursive `Frame` and `FrameData` types. This iteratively traverses as much of the
        // chain as needs to be deallocated, and repeatedly "pops" the head off the list. This
        // solves the problem, as when the time comes to actually deallocate the `FrameData`,
        // the `next` field will contain a `Frame::End`, or an `Arc` with additional references,
        // so the depth of recursive drops will be bounded.
        loop {
            if let Ok(arc_next) = self.next.get_mut() {
                if let Some(next_ref) = Arc::get_mut(arc_next) {
                    // This allows us to own the next Frame.
                    let next = mem::replace(next_ref, Frame::End);
                    if let Frame::Data(next_data) = next {
                        // Swap the current FrameData with the next one, allowing the current one
                        // to go out of scope.
                        *self = next_data;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
}

/// Builds a frame from the input iterator.
fn extract<I>(mut input: I) -> Arc<Frame<I>>
where
    I: Source,
    I::Item: Sample,
{
    let frame_len = input.current_frame_len();

    if frame_len == Some(0) {
        return Arc::new(Frame::End);
    }

    let channels = input.channels();
    let rate = input.sample_rate();
    let data: Vec<I::Item> = input
        .by_ref()
        .take(cmp::min(frame_len.unwrap_or(32768), 32768))
        .collect();

    if data.is_empty() {
        return Arc::new(Frame::End);
    }

    Arc::new(Frame::Data(FrameData {
        data,
        channels,
        rate,
        next: Mutex::new(Arc::new(Frame::Input(Mutex::new(Some(input))))),
    }))
}

impl<I> Buffered<I>
where
    I: Source,
    I::Item: Sample,
{
    /// Advances to the next frame.
    fn next_frame(&mut self) {
        let next_frame = {
            let mut next_frame_ptr = match &*self.current_frame {
                Frame::Data(FrameData { next, .. }) => next.lock().unwrap(),
                _ => unreachable!(),
            };

            let next_frame = match &**next_frame_ptr {
                Frame::Data(_) => next_frame_ptr.clone(),
                Frame::End => next_frame_ptr.clone(),
                Frame::Input(input) => {
                    let input = input.lock().unwrap().take().unwrap();
                    extract(input)
                }
            };

            *next_frame_ptr = next_frame.clone();
            next_frame
        };

        self.current_frame = next_frame;
        self.position_in_frame = 0;
    }
}

impl<I> Iterator for Buffered<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    #[inline]
    fn next(&mut self) -> Option<I::Item> {
        let current_sample;
        let advance_frame;

        match &*self.current_frame {
            Frame::Data(FrameData { data, .. }) => {
                current_sample = Some(data[self.position_in_frame]);
                self.position_in_frame += 1;
                advance_frame = self.position_in_frame >= data.len();
            }

            Frame::End => {
                current_sample = None;
                advance_frame = false;
            }

            Frame::Input(_) => unreachable!(),
        };

        if advance_frame {
            self.next_frame();
        }

        current_sample
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        // TODO:
        (0, None)
    }
}

// TODO: uncomment when `size_hint` is fixed
/*impl<I> ExactSizeIterator for Amplify<I> where I: Source + ExactSizeIterator, I::Item: Sample {
}*/

impl<I> Source for Buffered<I>
where
    I: Source,
    I::Item: Sample,
{
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        match &*self.current_frame {
            Frame::Data(FrameData { data, .. }) => Some(data.len() - self.position_in_frame),
            Frame::End => Some(0),
            Frame::Input(_) => unreachable!(),
        }
    }

    #[inline]
    fn channels(&self) -> u16 {
        match *self.current_frame {
            Frame::Data(FrameData { channels, .. }) => channels,
            Frame::End => 1,
            Frame::Input(_) => unreachable!(),
        }
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        match *self.current_frame {
            Frame::Data(FrameData { rate, .. }) => rate,
            Frame::End => 44100,
            Frame::Input(_) => unreachable!(),
        }
    }

    #[inline]
    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }
}

impl<I> Clone for Buffered<I>
where
    I: Source,
    I::Item: Sample,
{
    #[inline]
    fn clone(&self) -> Buffered<I> {
        Buffered {
            current_frame: self.current_frame.clone(),
            position_in_frame: self.position_in_frame,
            total_duration: self.total_duration,
        }
    }
}
