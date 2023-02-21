use bytes::{BufMut, BytesMut};

use super::mp4box::{Boxes, ParseBox};
use super::{BoxType, ParseError, ParsedBox, StblBox};

#[derive(Clone, Debug)]
pub struct MinfBox {
    pub children: Boxes,
}

const NAME: BoxType = BoxType::MINF;

impl MinfBox {
    pub fn stbl_mut(&mut self) -> Result<&mut StblBox, ParseError> {
        self.children.get_one_mut()
    }
}

impl ParseBox for MinfBox {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        let children = Boxes::parse(buf)?;
        Ok(Self { children })
    }

    fn box_type() -> BoxType {
        NAME
    }
}

impl ParsedBox for MinfBox {
    fn encoded_len(&self) -> u64 {
        self.children.encoded_len()
    }

    fn put_buf(&self, buf: &mut dyn BufMut) {
        self.children.put_buf(buf)
    }
}
