use bytes::{BufMut, BytesMut};

use super::mp4box::Boxes;
use super::{BoxType, MdiaBox, ParseBox, ParseError, ParsedBox};

#[derive(Clone, Debug)]
pub struct TrakBox {
    pub children: Boxes,
}

const NAME: BoxType = BoxType::TRAK;

impl TrakBox {
    pub fn mdia_mut(&mut self) -> Result<&mut MdiaBox, ParseError> {
        self.children.get_one_mut()
    }
}

impl ParseBox for TrakBox {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        let children = Boxes::parse(buf)?;
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
