#![allow(dead_code)]

use crate::{Sample, Source};
pub type ChannelBitmask = u64;

pub const FRONT_LEFT: ChannelBitmask = 0x1;
pub const FRONT_RIGHT: ChannelBitmask = 0x2;
pub const FRONT_CENTER: ChannelBitmask = 0x4;
pub const LFE: ChannelBitmask = 0x8;
pub const BACK_LEFT: ChannelBitmask = 0x10;
pub const BACK_RIGHT: ChannelBitmask = 0x20;
pub const FRONT_LEFT_CENTER: ChannelBitmask = 0x40;
pub const FRONT_RIGHT_CENTER: ChannelBitmask = 0x80;
pub const BACK_CENTER: ChannelBitmask = 0x100;
pub const SIDE_LEFT: ChannelBitmask = 0x200;
pub const SIDE_RIGHT: ChannelBitmask = 0x400;
pub const TOP_CENTER: ChannelBitmask = 0x800;
pub const TOP_FRONT_LEFT: ChannelBitmask = 0x1000;
pub const TOP_FRONT_CENTER: ChannelBitmask = 0x2000;
pub const TOP_FRONT_RIGHT: ChannelBitmask = 0x4000;
pub const TOP_BACK_LEFT: ChannelBitmask = 0x8000;
pub const TOP_BACK_CENTER: ChannelBitmask = 0x1_0000;
pub const TOP_BACK_RIGHT: ChannelBitmask = 0x2_0000;

pub const FRONT_LEFT_WIDE: ChannelBitmask = 0x4_0000;
pub const FRONT_RIGHT_WIDE: ChannelBitmask = 0x8_0000;

pub const AMBISONIC_W: ChannelBitmask = 0x10_0000;
pub const AMBISONIC_X: ChannelBitmask = 0x20_0000;
pub const AMBISONIC_Y: ChannelBitmask = 0x40_0000;
pub const AMBISONIC_Z: ChannelBitmask = 0x80_0000;
pub const AMBISONIC_R: ChannelBitmask = 0x100_0000;
pub const AMBISONIC_S: ChannelBitmask = 0x200_0000;
pub const AMBISONIC_T: ChannelBitmask = 0x400_0000;
pub const AMBISONIC_U: ChannelBitmask = 0x800_0000;
pub const AMBISONIC_V: ChannelBitmask = 0x1000_0000;
pub const AMBISONIC_K: ChannelBitmask = 0x2000_0000;
pub const AMBISONIC_L: ChannelBitmask = 0x4000_0000;
pub const AMBISONIC_M: ChannelBitmask = 0x8000_0000;
pub const AMBISONIC_N: ChannelBitmask = 0x1_0000_0000;
pub const AMBISONIC_O: ChannelBitmask = 0x2_0000_0000;
pub const AMBISONIC_P: ChannelBitmask = 0x4_0000_0000;
pub const AMBISONIC_Q: ChannelBitmask = 0x8_0000_0000;

pub const MATRIX_LEFT_TOTAL: ChannelBitmask = 0x10_0000_0000;
pub const MATRIX_RIGHT_TOTAL: ChannelBitmask = 0x20_0000_0000;

pub const UNDEFINED: ChannelBitmask = 0x0;
pub const STEREO: ChannelBitmask = FRONT_LEFT ^ FRONT_RIGHT;
pub const LCR: ChannelBitmask = FRONT_LEFT ^ FRONT_CENTER ^ FRONT_RIGHT;
pub const LCRS: ChannelBitmask = LCR ^ BACK_CENTER;
pub const SURROUND_51: ChannelBitmask =
    FRONT_LEFT ^ FRONT_RIGHT ^ FRONT_CENTER ^ LFE ^ BACK_LEFT ^ BACK_RIGHT;
pub const SURROUND_71: ChannelBitmask = SURROUND_51 ^ SIDE_LEFT ^ SIDE_RIGHT;

pub const AMBISONIC_O1: ChannelBitmask = AMBISONIC_W ^ AMBISONIC_X ^ AMBISONIC_Y ^ AMBISONIC_Z;
pub const AMBISONIC_O2: ChannelBitmask =
    AMBISONIC_O1 ^ AMBISONIC_R ^ AMBISONIC_S ^ AMBISONIC_T ^ AMBISONIC_U ^ AMBISONIC_V;
pub const AMBISONIC_O3: ChannelBitmask = AMBISONIC_O2
    ^ AMBISONIC_K
    ^ AMBISONIC_L
    ^ AMBISONIC_M
    ^ AMBISONIC_N
    ^ AMBISONIC_O
    ^ AMBISONIC_P
    ^ AMBISONIC_Q;

/// A trait for [`Source`]'s that provide a channel bitmask.
///
/// Sources providing more than one channel of audio may be providing their channels in a
/// particular format for surround sound presentation (e.g. 5.1 surround). The
/// `SourceChannelBitmask` trait defines methods for a `Source` to inform a client the speaker
/// assignment or encoding component for each channel.
///
/// The trait uses [`ChannelBitmask`] values to describe component assignments for channels in an
/// implementing `Source`. `ChannelBitmask` constants define a single bit in a `u64` for each
/// possible component.
///
/// The presence of a given component in the source's output is signified by the setting of its
/// corresponding bit, and components are ordered in the Source's iteration according to their
/// corresponding `ChannelBitmask` value.
///
/// For example: if a source is generating a six-channel stream of samples, it's `channels()`
/// method will return "6" and its its [`SourceChannelBitmask::channel_bitmask()`] will return
/// a bitmask equal to "0b111111" or `FRONT_LEFT ^ FRONT_RIGHT ^ FRONT_CENTER ^ LFE ^ BACK_LEFT ^
/// BACK_RIGHT` (a constant `SURROUND_51` is provided that is equal to this.) The source will then
/// return its samples in the numerical order of these bitmasks: Left, Right, Center, LFE, Back
/// Left and Back Right.
trait SourceChannelBitmask: Source
where
    Self::Item: Sample,
{
    /// The [`ChannelBitmask`] of the `Source`.
    fn channel_bitmask(&self) -> ChannelBitmask;

}

/// A `Source` for adding a channel bitmask to a preexisting source.
///
/// This `Source` only adds the `SourceChannelBitmask` trait methods, allowing the inner source to
/// broadcast the given bitmask metadata. It otherwise does nothing to the source's samples and
/// refers all source methods back to the inner input. This source is provided as a convenience to
/// add a channel bitmask to a source that does not support it.
pub struct ChannelBitmaskAdapter<I> {
    input: I,
    channel_bitmask: ChannelBitmask,
}

impl<I> ChannelBitmaskAdapter<I> {
    fn new(input: I, channel_bitmask: ChannelBitmask) -> Self {
        Self {
            input,
            channel_bitmask,
        }
    }
}

impl<I> Source for ChannelBitmaskAdapter<I>
where
    I: Source,
    I::Item: Sample,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.input.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.input.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        self.input.total_duration()
    }
}

impl<I> Iterator for ChannelBitmaskAdapter<I>
where
    I: Source,
    I::Item: Sample,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.input.next()
    }
}

impl<I> SourceChannelBitmask for ChannelBitmaskAdapter<I>
where
    I: Source,
    I::Item: Sample,
{
    fn channel_bitmask(&self) -> ChannelBitmask {
        self.channel_bitmask
    }
}
