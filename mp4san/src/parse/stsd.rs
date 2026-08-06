#![allow(missing_docs)]

use crate::error::Result;

use super::error::ParseResultExt;
use super::mp4box::Boxes;
use super::{BoxType, ParseBox, ParseError, ParsedBox};

#[derive(Clone, Debug, ParseBox, ParsedBox)]
#[box_type = "stsd"]
pub struct StsdBox {
    children: Boxes,
}

const NAME: BoxType = BoxType::STSD;

impl StsdBox {
    #[cfg(test)]
    pub(crate) fn with_children<C: Into<Boxes>>(children: C) -> Self {
        Self { children: children.into() }
    }

    pub fn stsd_mut(&mut self) -> Result<&mut StsdBox, ParseError> {
        self.children.get_one_mut().while_parsing_child(NAME, BoxType::HEV1)
    }
}
