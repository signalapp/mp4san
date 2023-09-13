#![allow(missing_docs)]

use derive_more::{Deref, DerefMut};

use super::mp4box::Boxes;
use super::{MdiaBox, ParseBox, ParseBoxes, ParseError, ParsedBox, StblCoRef, StblCoRefMut, TkhdBox};
use crate::error::Result;

#[derive(Clone, Debug, Deref, DerefMut, ParseBox, ParsedBox)]
#[box_type = "trak"]
pub struct TrakBox {
    pub children: Boxes<TrakChildren>,
}

#[non_exhaustive]
#[derive(Clone, Debug, ParseBoxes)]
#[box_type = "trak"]
pub struct TrakChildren {
    pub header: TkhdBox,
    pub media: MdiaBox,
}

impl TrakBox {
    pub fn with_children(children: TrakChildren) -> Result<Self, ParseError> {
        Ok(Self { children: Boxes::new(children, [])? })
    }

    pub fn co(&self) -> StblCoRef<'_> {
        let media = self.parsed().media;
        let info = media.parsed().info;
        let samples = info.parsed().samples;
        samples.parsed().chunk_offsets
    }

    pub fn co_mut(&mut self) -> StblCoRefMut<'_> {
        let media = self.parsed_mut().media;
        let info = media.parsed_mut().info;
        let samples = info.parsed_mut().samples;
        samples.parsed_mut().chunk_offsets
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl TrakBox {
        pub(crate) fn dummy() -> Self {
            Self::new(TkhdBox::dummy(), MdiaBox::dummy()).unwrap()
        }

        pub(crate) fn new(header: TkhdBox, media: MdiaBox) -> Result<Self, ParseError> {
            Self::with_children(TrakChildren { header, media })
        }
    }
}
