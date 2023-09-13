#![allow(missing_docs)]

use std::num::NonZeroU32;

use fixed::types::{I16F16, I8F8};

use super::{ConstFullBoxHeader, ConstU16, ConstU32, Mp4Transform, ParseBox, ParsedBox};

#[derive(Clone, Debug, ParseBox, ParsedBox)]
#[box_type = "mvhd"]
pub enum MvhdBox {
    V1 {
        _parsed_header: ConstFullBoxHeader<1>,
        creation_time: u64,
        modification_time: u64,
        timescale: u32,
        duration: u64,
        rate: I16F16,
        volume: I8F8,
        _reserved: ConstU16,
        _reserved_2: [ConstU32; 2],
        matrix: Mp4Transform,
        _pre_defined: [ConstU32; 6],
        next_track_id: NonZeroU32,
    },
    V0 {
        _parsed_header: ConstFullBoxHeader,
        creation_time: u32,
        modification_time: u32,
        timescale: u32,
        duration: u32,
        rate: I16F16,
        volume: I8F8,
        _reserved: ConstU16,
        _reserved_2: [ConstU32; 2],
        matrix: Mp4Transform,
        _pre_defined: [ConstU32; 6],
        next_track_id: NonZeroU32,
    },
}

impl MvhdBox {
    pub const DURATION_UNDETERMINED_V0: u32 = u32::MAX;
    pub const DURATION_UNDETERMINED_V1: u64 = u64::MAX;
    pub const NEXT_TRACK_ID_UNDETERMINED: NonZeroU32 = {
        let Some(value) = NonZeroU32::new(u32::MAX) else {
            unreachable!()
        };
        value
    };
}

#[cfg(test)]
mod test {
    use super::*;

    impl MvhdBox {
        pub(crate) fn dummy() -> Self {
            Self::V1 {
                _parsed_header: Default::default(),
                creation_time: 0,
                modification_time: 0,
                timescale: 0,
                duration: Self::DURATION_UNDETERMINED_V1,
                rate: 1i16.into(),
                volume: 1.into(),
                _reserved: Default::default(),
                _reserved_2: Default::default(),
                matrix: Default::default(),
                _pre_defined: Default::default(),
                next_track_id: Self::NEXT_TRACK_ID_UNDETERMINED,
            }
        }
    }
}
