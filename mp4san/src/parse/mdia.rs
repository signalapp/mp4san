#![allow(missing_docs)]

use derive_more::{Deref, DerefMut};

use crate::error::Result;

use super::mp4box::Boxes;
use super::{MinfBox, ParseBox, ParseBoxes, ParseError, ParsedBox};

#[derive(Clone, Debug, Deref, DerefMut, ParseBox, ParsedBox)]
#[box_type = "mdia"]
pub struct MdiaBox {
    pub children: Boxes<MdiaChildren>,
}

#[non_exhaustive]
#[derive(Clone, Debug, ParseBoxes)]
#[box_type = "mdia"]
pub struct MdiaChildren {
    pub info: MinfBox,
}

impl MdiaBox {
    #[cfg(test)]
    pub(crate) fn new(info: MinfBox) -> Result<Self, ParseError> {
        Self::with_children(MdiaChildren { info })
    }

    pub fn with_children(children: MdiaChildren) -> Result<Self, ParseError> {
        Ok(Self { children: Boxes::new(children, [])? })
    }
}
