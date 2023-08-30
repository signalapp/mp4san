#![allow(missing_docs)]

use derive_more::{Deref, DerefMut};

use crate::error::Result;

use super::mp4box::Boxes;
use super::{ParseBox, ParseBoxes, ParseError, ParsedBox, StblBox};

#[derive(Clone, Debug, Deref, DerefMut, ParseBox, ParsedBox)]
#[box_type = "minf"]
pub struct MinfBox {
    pub children: Boxes<MinfChildren>,
}

#[non_exhaustive]
#[derive(Clone, Debug, ParseBoxes)]
#[box_type = "minf"]
pub struct MinfChildren {
    pub samples: StblBox,
}

impl MinfBox {
    #[cfg(test)]
    pub(crate) fn new(samples: StblBox) -> Result<Self, ParseError> {
        Self::with_children(MinfChildren { samples })
    }

    pub fn with_children(children: MinfChildren) -> Result<Self, ParseError> {
        Ok(Self { children: Boxes::new(children, [])? })
    }
}
