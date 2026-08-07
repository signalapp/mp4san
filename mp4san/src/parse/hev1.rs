#![allow(missing_docs)]

use crate::error::Result;

use super::error::ParseResultExt;
use super::mp4box::Boxes;
use super::{BoxType, ParseBox, ParseError, ParsedBox};

#[derive(Clone, Debug, ParseBox, ParsedBox)]
#[box_type = "stsd"]
pub struct Hev1Box {
    children: Boxes,
}

const NAME: BoxType = BoxType::HEV1;

impl Hev1Box {
    #[cfg(test)]
    pub(crate) fn with_children<C: Into<Boxes>>(children: C) -> Self {
        Self { children: children.into() }
    }

    pub fn hev1_mut(&mut self) -> Result<&mut Hev1Box, ParseError> {
        println!("====> HEV1_MUT");
        self.children.get_one_mut().while_parsing_child(NAME, BoxType::HEV1)
    }
}
