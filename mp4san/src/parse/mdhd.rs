#![allow(missing_docs)]

use super::{ConstFullBoxHeader, ConstU16, ParseBox, ParsedBox};

#[derive(Clone, Debug, ParseBox, ParsedBox)]
#[box_type = "mdhd"]
pub enum MdhdBox {
    V1 {
        _parsed_header: ConstFullBoxHeader<1>,
        creation_time: u64,
        modification_time: u64,
        timescale: u32,
        duration: u64,
        languages: [u8; 2],
        _pre_defined: ConstU16,
    },
    V0 {
        _parsed_header: ConstFullBoxHeader,
        creation_time: u32,
        modification_time: u32,
        timescale: u32,
        duration: u32,
        languages: [u8; 2],
        _pre_defined: ConstU16,
    },
}

impl MdhdBox {
    pub const DURATION_UNDETERMINED_V0: u32 = u32::MAX;
    pub const DURATION_UNDETERMINED_V1: u64 = u64::MAX;

    #[cfg(test)]
    pub(crate) fn dummy() -> Self {
        Self::V1 {
            _parsed_header: ConstFullBoxHeader,
            creation_time: 0,
            modification_time: 0,
            timescale: 0,
            duration: Self::DURATION_UNDETERMINED_V1,
            languages: [0; 2],
            _pre_defined: Default::default(),
        }
    }
}
