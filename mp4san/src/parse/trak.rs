#![allow(missing_docs)]

use crate::error::Result;

use super::error::ParseResultExt;
use super::mp4box::Boxes;
use super::{BoxType, MdiaBox, ParseBox, ParseError, ParsedBox, StblCoMut};

#[derive(Clone, Debug, ParseBox, ParsedBox)]
#[box_type = "trak"]
pub struct TrakBox {
    children: Boxes,
}

const NAME: BoxType = BoxType::TRAK;

impl TrakBox {
    #[cfg(test)]
    pub(crate) fn with_children<C: Into<Boxes>>(children: C) -> Self {
        Self { children: children.into() }
    }

    pub fn co_mut(&mut self) -> Result<StblCoMut<'_>, ParseError> {
        self.mdia_mut()?.minf_mut()?.stbl_mut()?.co_mut()
    }

    pub fn mdia_mut(&mut self) -> Result<&mut MdiaBox, ParseError> {
        self.children.get_one_mut().while_parsing_child(NAME, BoxType::MDIA)
    }
}
