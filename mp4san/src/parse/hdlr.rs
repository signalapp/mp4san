#![allow(missing_docs)]

use super::{ConstFullBoxHeader, ConstU32, FourCC, Mp4String, ParseBox, ParsedBox};

#[derive(Clone, Debug, ParseBox, ParsedBox)]
#[box_type = "hdlr"]
pub struct HdlrBox {
    _parsed_header: ConstFullBoxHeader,
    _pre_defined: ConstU32,
    handler_type: FourCC,
    _reserved: [ConstU32; 3],
    name: Mp4String,
}

impl HdlrBox {
    define_fourcc_lower!(VIDE, SOUN, HINT, AUXV, NULL);
}

#[cfg(test)]
mod test {
    use super::*;

    impl HdlrBox {
        pub(crate) fn dummy() -> Self {
            Self {
                _parsed_header: Default::default(),
                _pre_defined: Default::default(),
                handler_type: Self::NULL,
                _reserved: Default::default(),
                name: Default::default(),
            }
        }
    }
}
