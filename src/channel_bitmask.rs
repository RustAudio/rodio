
pub type ChannelBitmask = u64;

const FRONT_LEFT: ChannelBitmask = 0x1;
const FRONT_RIGHT: ChannelBitmask = 0x2;
const FRONT_CENTER: ChannelBitmask = 0x4;
const LFE: ChannelBitmask = 0x8;
const BACK_LEFT: ChannelBitmask = 0x10;
const BACK_RIGHT: ChannelBitmask = 0x20;
const FRONT_LEFT_CENTER: ChannelBitmask = 0x40;
const FRONT_RIGHT_CENTER: ChannelBitmask = 0x80;
const REAR_CENTER: ChannelBitmask = 0x100;
const SIDE_LEFT: ChannelBitmask = 0x200;
const SIDE_RIGHT: ChannelBitmask = 0x400;
const TOP_CENTER: ChannelBitmask = 0x800;
const TOP_FRONT_LEFT: ChannelBitmask = 0x1000;
const TOP_FRONT_CENTER: ChannelBitmask = 0x2000;
const TOP_FRONT_RIGHT: ChannelBitmask = 0x4000;
const TOP_BACK_LEFT: ChannelBitmask = 0x8000;
const TOP_BACK_CENTER: ChannelBitmask = 0x10000;
const TOP_BACK_RIGHT: ChannelBitmask = 0x20000;

/// Left total. The left channel of phase-matrix encoded audio, as in Dolby Stereo.
const LEFT_TOTAL: ChannelBitmask = 0x40000;

/// Right total. The right channel of phase-matrix encoded audio, as in Dolby Stereo.
const RIGHT_TOTAL: ChannelBitmask = 0x80000;

const AMBISONIC_W: ChannelBitmask = 0x100000;
const AMBISONIC_X: ChannelBitmask = 0x200000;
const AMBISONIC_Y: ChannelBitmask = 0x400000;
const AMBISONIC_Z: ChannelBitmask = 0x800000;
const AMBISONIC_R: ChannelBitmask = 0x1000000;
const AMBISONIC_S: ChannelBitmask = 0x2000000;
const AMBISONIC_T: ChannelBitmask = 0x4000000;
const AMBISONIC_U: ChannelBitmask = 0x8000000;
const AMBISONIC_V: ChannelBitmask = 0x10000000;
const AMBISONIC_K: ChannelBitmask = 0x20000000;
const AMBISONIC_L: ChannelBitmask = 0x40000000;
const AMBISONIC_M: ChannelBitmask = 0x80000000;
const AMBISONIC_N: ChannelBitmask = 0x100000000;
const AMBISONIC_O: ChannelBitmask = 0x200000000;
const AMBUSONIC_P: ChannelBitmask = 0x400000000;
const AMBISONIC_Q: ChannelBitmask = 0x800000000;

const UNDEFINED: ChannelBitmask = 0x0;
const STEREO: ChannelBitmask = FRONT_LEFT ^ FRONT_RIGHT;
const SURROUND_51: ChannelBitmask = FRONT_LEFT ^ FRONT_RIGHT ^ FRONT_CENTER ^ LFE ^ BACK_LEFT ^ BACK_RIGHT;
const SURROUND_71: ChannelBitmask = SURROUND_51 ^ SIDE_LEFT ^ SIDE_RIGHT;
