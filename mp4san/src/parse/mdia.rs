#![allow(missing_docs)]

use crate::error::Result;

use super::error::ParseResultExt;
use super::mp4box::Boxes;
use super::{BoxType, MinfBox, ParseBox, ParseError, ParsedBox};

#[derive(Clone, Debug, ParseBox, ParsedBox)]
#[box_type = "mdia"]
pub struct MdiaBox {
    pub children: Boxes,
}

const NAME: BoxType = BoxType::MDIA;

impl MdiaBox {
    #[cfg(test)]
    pub(crate) fn with_children<C: Into<Boxes>>(children: C) -> Self {
        Self { children: children.into() }
    }

    pub fn minf_mut(&mut self) -> Result<&mut MinfBox, ParseError> {
        self.children.get_one_mut().while_parsing_child(NAME, BoxType::MINF)
    }
}
