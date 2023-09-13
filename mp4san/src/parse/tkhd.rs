#![allow(missing_docs)]

use fixed::types::I8F8;

use super::{BoxFlags, ConstU16, ConstU32, ConstU8, Mp4Transform, ParseBox, ParsedBox};

#[derive(Clone, Debug, ParseBox, ParsedBox)]
#[box_type = "tkhd"]
pub enum TkhdBox {
    V1 {
        _version: ConstU8<1>,
        flags: BoxFlags,
        creation_time: u64,
        modification_time: u64,
        track_id: u32,
        _reserved: ConstU32,
        duration: u64,
        _reserved_2: [ConstU32; 2],
        layer: i16,
        alternate_group: i16,
        volume: I8F8,
        _reserved_3: ConstU16,
        matrix: Mp4Transform,
        width: u32,
        height: u32,
    },
    V0 {
        _version: ConstU8,
        flags: BoxFlags,
        creation_time: u32,
        modification_time: u32,
        track_id: u32,
        _reserved: ConstU32,
        duration: u32,
        _reserved_2: [ConstU32; 2],
        layer: i16,
        alternate_group: i16,
        volume: I8F8,
        _reserved_3: ConstU16,
        matrix: Mp4Transform,
        width: u32,
        height: u32,
    },
}

impl TkhdBox {
    pub const DURATION_UNDETERMINED_V0: u32 = u32::MAX;
    pub const DURATION_UNDETERMINED_V1: u64 = u64::MAX;
}

#[cfg(test)]
mod test {
    use super::*;

    impl TkhdBox {
        pub(crate) fn dummy() -> Self {
            Self::V1 {
                _version: Default::default(),
                flags: Default::default(),
                creation_time: 0,
                modification_time: 0,
                track_id: 0,
                _reserved: Default::default(),
                duration: Self::DURATION_UNDETERMINED_V1,
                _reserved_2: Default::default(),
                layer: 0,
                alternate_group: 0,
                volume: 1.into(),
                _reserved_3: Default::default(),
                matrix: Default::default(),
                width: 0,
                height: 0,
            }
        }
    }
}
