#![allow(missing_docs)]

use derive_more::{Deref, DerefMut};

use crate::error::Result;

use super::mp4box::Boxes;
use super::{MdhdBox, MinfBox, ParseBox, ParseBoxes, ParseError, ParsedBox};

#[derive(Clone, Debug, Deref, DerefMut, ParseBox, ParsedBox)]
#[box_type = "mdia"]
pub struct MdiaBox {
    pub children: Boxes<MdiaChildren>,
}

#[non_exhaustive]
#[derive(Clone, Debug, ParseBoxes)]
#[box_type = "mdia"]
pub struct MdiaChildren {
    pub header: MdhdBox,
    pub info: MinfBox,
}

impl MdiaBox {
    pub fn with_children(children: MdiaChildren) -> Result<Self, ParseError> {
        Ok(Self { children: Boxes::new(children, [])? })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl MdiaBox {
        pub(crate) fn dummy() -> Self {
            Self::new(MdhdBox::dummy(), MinfBox::dummy()).unwrap()
        }

        pub(crate) fn new(header: MdhdBox, info: MinfBox) -> Result<Self, ParseError> {
            Self::with_children(MdiaChildren { header, info })
        }
    }
}
