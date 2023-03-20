#![allow(missing_docs)]

use bytes::{BufMut, BytesMut};
use error_stack::Result;

use super::error::ParseResultExt;
use super::mp4box::{Boxes, ParseBox};
use super::{BoxType, MinfBox, ParseError, ParsedBox};

#[derive(Clone, Debug)]
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

impl ParseBox for MdiaBox {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        let children = Boxes::parse(buf).while_parsing_field(NAME, "children")?;
        Ok(Self { children })
    }

    fn box_type() -> BoxType {
        NAME
    }
}

impl ParsedBox for MdiaBox {
    fn encoded_len(&self) -> u64 {
        self.children.encoded_len()
    }

    fn put_buf(&self, buf: &mut dyn BufMut) {
        self.children.put_buf(buf)
    }
}
