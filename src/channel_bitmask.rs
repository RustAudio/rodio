#![allow(dead_code)]

use crate::{Sample, Source};

/// Channel Bitmask
///
/// Each bit in a channel bitmask indicates a different speaker or channel component in a
/// multichannel audio stream. Multichannel streams set each bit "1" for the corresponding
/// components they provide samples for, and then provide them in the order of the place-value of
/// the bit, right-to-left/LSB-to-MSB.
///
/// # Note on the Immersive Audio
///
/// Constants from `0x0001...0x800_0000` are for theatrical presentations, for presentations in
/// theaters and equipped home theaters. For immersive presentations like Oculus, Apple Vision Pro
/// etc. the presentation of these components is undefined.
///
/// 0x0001 ... 0x2_0000
///     Channel bitmasks 0x1 through 0x2_0000 are identical to speaker assignements in the
///     Microsoft Wave WAVEFORMATEX channel bitmask.
///
///     BACK_  and SIDE_ channel assignments correspond to surround assignments in the
///     respective Dolby and DTS channel assignments. These are arrays of speakers arround the
///     listener, see the DIRECT assignments for point surrounds.
///
///     TOP_FRONT_ and TOP_BACK_ channel assignments correspond to overhead surround assignments
///     ahead and behind the listener, accoding to Dolby Home Atmos 7.1.4 standards.
///
/// 0x4_0000 ... 0x20_0000
///     These speaker assignents are required to complete a full Dolby Atmos 9.1.6 bed.
///
/// 0x40_0000 and 0x80_0000
///     These "DIRECT" speaker assigments are required for IMAX and TMH 10.2.
///
/// 0x100_0000 and 0x200_0000
///     We've dedicated these two bits for left-total and right-total components, for use with
///     matrix-encoded formats. Without further qualification, channels mapped to these code
///     points will be interpreted as Dolby Pro Logic 2 Left Total and Right Total components, but
///     other possibilities here could be CBS SQ, Sansui QS or CD-4.
///
/// 0x400_0000 and 0x800_0000
///     These are head-locked left and right compoonents of an Ambisonic presentation. These are
///     presented a headset without spatialization simultaneous with...
///
/// 0x1000_0000 ... 0x800_0000_0000
///     These are all Ambisonic components in FuMa order.
pub type ChannelBitmask = u64;

// WAVEFORMATEX
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

// For Dolby Atmos
pub const FRONT_LEFT_WIDE: ChannelBitmask = 0x4_0000;
pub const FRONT_RIGHT_WIDE: ChannelBitmask = 0x8_0000;
pub const TOP_SIDE_LEFT: ChannelBitmask = 0x10_0000;
pub const TOP_SIDE_RIGHT: ChannelBitmask = 0x20_0000;

// For IMAX and TMH
pub const BACK_DIRECT_LEFT: ChannelBitmask = 0x40_0000;
pub const BACK_DIRECT_RIGHT: ChannelBitmask = 0x80_0000;

// Matrix channel assignments are required for Dolby Stereo, Dolby Pro Logic 2 and compatibility with
// ITU Audio Definition Model.
pub const MATRIX_LEFT_TOTAL: ChannelBitmask = 0x100_0000;
pub const MATRIX_RIGHT_TOTAL: ChannelBitmask = 0x200_0000;

// Ambisonic components
//
// Head-locked mixes:
pub const AMBISONIC_HEADLOCKED_L: ChannelBitmask = 0x400_000;
pub const AMBISONIC_HEADLOCKED_R: ChannelBitmask = 0x800_000;

// Ambisonic components in FuMa order
pub const AMBISONIC_W: ChannelBitmask = 0x1000_0000;
pub const AMBISONIC_X: ChannelBitmask = 0x2000_0000;
pub const AMBISONIC_Y: ChannelBitmask = 0x4000_0000;
pub const AMBISONIC_Z: ChannelBitmask = 0x8000_0000;
pub const AMBISONIC_R: ChannelBitmask = 0x1_0000_0000;
pub const AMBISONIC_S: ChannelBitmask = 0x2_0000_0000;
pub const AMBISONIC_T: ChannelBitmask = 0x4_0000_0000;
pub const AMBISONIC_U: ChannelBitmask = 0x8_0000_0000;
pub const AMBISONIC_V: ChannelBitmask = 0x10_0000_0000;
pub const AMBISONIC_K: ChannelBitmask = 0x20_0000_0000;
pub const AMBISONIC_L: ChannelBitmask = 0x40_0000_0000;
pub const AMBISONIC_M: ChannelBitmask = 0x80_0000_0000;
pub const AMBISONIC_N: ChannelBitmask = 0x100_0000_0000;
pub const AMBISONIC_O: ChannelBitmask = 0x200_0000_0000;
pub const AMBISONIC_P: ChannelBitmask = 0x400_0000_0000;
pub const AMBISONIC_Q: ChannelBitmask = 0x800_0000_0000;

/// Undefined channel format. Use this if the format is unknown or if you are using another method
/// to inform clients about channel components. Monoaural sources may use this or use
/// `FRONT_CENTER`.
pub const UNDEFINED: ChannelBitmask = 0x0;

/// Left-right stereo, for both speakers and headphones.
pub const STEREO: ChannelBitmask = FRONT_LEFT ^ FRONT_RIGHT;

/// Stereo with a hard center.
pub const LCR: ChannelBitmask = FRONT_LEFT ^ FRONT_CENTER ^ FRONT_RIGHT;

/// Four-channel surround sound, as like classic discrete Dolby Stereo.
pub const LCRS: ChannelBitmask = LCR ^ BACK_CENTER;

/// 5.0 surround.
pub const SURROUND_5_0: ChannelBitmask =
    FRONT_LEFT ^ FRONT_RIGHT ^ FRONT_CENTER ^ BACK_LEFT ^ BACK_RIGHT;

/// 5.1 surround, with LFE.
pub const SURROUND_5_1: ChannelBitmask = SURROUND_5_0 ^ LFE;

/// 7.0 surround sound, with four surround channels.
pub const SURROUND_7_0: ChannelBitmask = SURROUND_5_0 ^ SIDE_LEFT ^ SIDE_RIGHT;

/// 7.1 surround sound, with four surround channels and LFE, as like Dolby Surround 7.1.
pub const SURROUND_7_1: ChannelBitmask = SURROUND_7_0 ^ LFE;

/// 7.1 surround sound with five front channels and two surround channels, as like Sony Dynamic
/// Digital Sound (SDDS).
pub const SDDS_7_1: ChannelBitmask = SURROUND_5_1 ^ FRONT_LEFT_CENTER ^ FRONT_RIGHT_CENTER;

/// Dolby Atmos 5.1.2 Bed.
pub const ATMOS_5_1_2: ChannelBitmask = SURROUND_5_1 ^ TOP_SIDE_LEFT ^ TOP_SIDE_RIGHT;

/// Dolby Atmos 5.0.2 Bed.
pub const ATMOS_5_0_2: ChannelBitmask = SURROUND_5_0 ^ TOP_SIDE_LEFT ^ TOP_SIDE_RIGHT;

/// Dolby Atmos 7.1.4 Bed.
pub const ATMOS_7_1_4: ChannelBitmask =
    SURROUND_5_1 ^ TOP_FRONT_LEFT ^ TOP_FRONT_RIGHT ^ TOP_BACK_LEFT ^ TOP_BACK_RIGHT;

/// Dolby Atmos 7.1.2 Bed.
pub const ATMOS_7_1_2: ChannelBitmask = SURROUND_5_1 ^ TOP_SIDE_LEFT ^ TOP_SIDE_RIGHT;

/// Dolby Atmos 7.0.2 Bed.
pub const ATMOS_7_0_2: ChannelBitmask = SURROUND_7_0 ^ TOP_SIDE_LEFT ^ TOP_SIDE_RIGHT;

/// Dolby Atmos 9.1.6 Bed.
pub const ATMOS_9_1_6: ChannelBitmask = SURROUND_7_1
    ^ FRONT_LEFT_WIDE
    ^ FRONT_RIGHT_WIDE
    ^ TOP_FRONT_LEFT
    ^ TOP_FRONT_RIGHT
    ^ TOP_BACK_LEFT
    ^ TOP_BACK_RIGHT
    ^ TOP_SIDE_LEFT
    ^ TOP_SIDE_RIGHT;

/// First-order Ambisonic components, WXYZ.
pub const AMBISONIC_O1: ChannelBitmask = AMBISONIC_W ^ AMBISONIC_X ^ AMBISONIC_Y ^ AMBISONIC_Z;

/// Second-order Ambisonic components, WXYZ+RSTUV.
pub const AMBISONIC_O2: ChannelBitmask =
    AMBISONIC_O1 ^ AMBISONIC_R ^ AMBISONIC_S ^ AMBISONIC_T ^ AMBISONIC_U ^ AMBISONIC_V;

/// Third-order Ambisonic components. WXYZ+RSTUV+KLMNOPQ.
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

/// Return a source wrapping `input` that implements the `ChannelBitmask` trait.
///
/// # Panics
///
/// If `channel_bitmask.count_ones()` is not equal to `input.channels()`.
pub fn add_channel_mask<I>(input: I, channel_bitmask: ChannelBitmask) -> ChannelBitmaskAdapter<I>
where
    I: Source,
    I::Item: Sample,
{
    assert!(
        input.channels() == channel_bitmask.count_ones() as u16,
        "Count of 1 bits in channel bitmask do not match count of channels in input!"
    );
    ChannelBitmaskAdapter::new(input, channel_bitmask)
}

/// A `Source` for adding a channel bitmask to a preexisting source.
///
/// This `Source` only adds the `SourceChannelBitmask` trait methods, allowing the inner source to
/// provide the given bitmask metadata. It otherwise does nothing to the source's samples and
/// refers all `Source` methods back to the inner input. This source is provided as a convenience
/// to add a channel bitmask to a source that does not support it.
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

    /// Get the input source, consuming the adapter
    fn into_inner(self) -> I {
        self.input
    }

    /// Get a mutable reference to the inner source
    fn inner_mut(&mut self) -> &I {
        &mut self.input
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
