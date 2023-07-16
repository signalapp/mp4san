#![allow(missing_docs)]

use crate::error::Result;

use super::error::ParseResultExt;
use super::mp4box::Boxes;
use super::{BoxType, ParseBox, ParseError, ParsedBox, StblBox};

#[derive(Clone, Debug, ParseBox, ParsedBox)]
#[box_type = "minf"]
pub struct MinfBox {
    children: Boxes,
}

const NAME: BoxType = BoxType::MINF;

impl MinfBox {
    #[cfg(test)]
    pub(crate) fn with_children<C: Into<Boxes>>(children: C) -> Self {
        Self { children: children.into() }
    }

    pub fn stbl_mut(&mut self) -> Result<&mut StblBox, ParseError> {
        self.children.get_one_mut().while_parsing_child(NAME, BoxType::STBL)
    }
}
