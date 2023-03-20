#![allow(missing_docs)]

use bytes::{BufMut, BytesMut};
use error_stack::Result;

use super::error::ParseResultExt;
use super::mp4box::Boxes;
use super::{BoxType, MdiaBox, ParseBox, ParseError, ParsedBox};

#[derive(Clone, Debug)]
pub struct TrakBox {
    children: Boxes,
}

const NAME: BoxType = BoxType::TRAK;

impl TrakBox {
    #[cfg(test)]
    pub(crate) fn with_children<C: Into<Boxes>>(children: C) -> Self {
        Self { children: children.into() }
    }

    pub fn mdia_mut(&mut self) -> Result<&mut MdiaBox, ParseError> {
        self.children.get_one_mut().while_parsing_child(NAME, BoxType::MDIA)
    }
}

impl ParseBox for TrakBox {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        let children = Boxes::parse(buf).while_parsing_field(NAME, "children")?;
        Ok(Self { children })
    }

    fn box_type() -> BoxType {
        NAME
    }
}

impl ParsedBox for TrakBox {
    fn encoded_len(&self) -> u64 {
        self.children.encoded_len()
    }

    fn put_buf(&self, out: &mut dyn BufMut) {
        self.children.put_buf(out)
    }
}
