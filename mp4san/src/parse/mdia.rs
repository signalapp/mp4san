use bytes::{BufMut, BytesMut};

use super::mp4box::{Boxes, ParseBox};
use super::{BoxType, MinfBox, ParseError, ParsedBox};

#[derive(Clone, Debug)]
pub struct MdiaBox {
    pub children: Boxes,
}

const NAME: BoxType = BoxType::MDIA;

impl MdiaBox {
    pub fn minf_mut(&mut self) -> Result<&mut MinfBox, ParseError> {
        self.children.get_one_mut()
    }
}

impl ParseBox for MdiaBox {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        let children = Boxes::parse(buf)?;
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
