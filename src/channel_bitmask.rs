pub type ChannelBitmask = u64;

pub const FRONT_LEFT: ChannelBitmask = 0x1;
pub const FRONT_RIGHT: ChannelBitmask = 0x2;
pub const FRONT_CENTER: ChannelBitmask = 0x4;
pub const LFE: ChannelBitmask = 0x8;
pub const BACK_LEFT: ChannelBitmask = 0x10;
pub const BACK_RIGHT: ChannelBitmask = 0x20;
pub const FRONT_LEFT_CENTER: ChannelBitmask = 0x40;
pub const FRONT_RIGHT_CENTER: ChannelBitmask = 0x80;
pub const REAR_CENTER: ChannelBitmask = 0x100;
pub const SIDE_LEFT: ChannelBitmask = 0x200;
pub const SIDE_RIGHT: ChannelBitmask = 0x400;
pub const TOP_CENTER: ChannelBitmask = 0x800;
pub const TOP_FRONT_LEFT: ChannelBitmask = 0x1000;
pub const TOP_FRONT_CENTER: ChannelBitmask = 0x2000;
pub const TOP_FRONT_RIGHT: ChannelBitmask = 0x4000;
pub const TOP_BACK_LEFT: ChannelBitmask = 0x8000;
pub const TOP_BACK_CENTER: ChannelBitmask = 0x10000;
pub const TOP_BACK_RIGHT: ChannelBitmask = 0x20000;

/// Left total. The left channel of phase-matrix encoded audio, as in Dolby Stereo.
pub const LEFT_TOTAL: ChannelBitmask = 0x40000;

/// Right total. The right channel of phase-matrix encoded audio, as in Dolby Stereo.
pub const RIGHT_TOTAL: ChannelBitmask = 0x80000;

pub const AMBISONIC_W: ChannelBitmask = 0x100000;
pub const AMBISONIC_X: ChannelBitmask = 0x200000;
pub const AMBISONIC_Y: ChannelBitmask = 0x400000;
pub const AMBISONIC_Z: ChannelBitmask = 0x800000;
pub const AMBISONIC_R: ChannelBitmask = 0x1000000;
pub const AMBISONIC_S: ChannelBitmask = 0x2000000;
pub const AMBISONIC_T: ChannelBitmask = 0x4000000;
pub const AMBISONIC_U: ChannelBitmask = 0x8000000;
pub const AMBISONIC_V: ChannelBitmask = 0x10000000;
pub const AMBISONIC_K: ChannelBitmask = 0x20000000;
pub const AMBISONIC_L: ChannelBitmask = 0x40000000;
pub const AMBISONIC_M: ChannelBitmask = 0x80000000;
pub const AMBISONIC_N: ChannelBitmask = 0x100000000;
pub const AMBISONIC_O: ChannelBitmask = 0x200000000;
pub const AMBISONIC_P: ChannelBitmask = 0x400000000;
pub const AMBISONIC_Q: ChannelBitmask = 0x800000000;

pub const UNDEFINED: ChannelBitmask = 0x0;
pub const STEREO: ChannelBitmask = FRONT_LEFT ^ FRONT_RIGHT;
pub const LCR: ChannelBitmask = FRONT_LEFT ^ FRONT_CENTER ^ FRONT_RIGHT;
pub const LCRS: ChannelBitmask = LCR ^ REAR_CENTER;
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
