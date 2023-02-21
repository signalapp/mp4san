use bytes::{BufMut, BytesMut};

use super::mp4box::Boxes;
use super::{BoxType, ParseBox, ParseError, ParsedBox, TrakBox};

#[derive(Clone, Debug, Default)]
pub struct MoovBox {
    children: Boxes,
}

const NAME: BoxType = BoxType::MOOV;

impl MoovBox {
    pub fn traks(&mut self) -> impl Iterator<Item = Result<&mut TrakBox, ParseError>> + '_ {
        self.children.get_mut()
    }

    pub fn encoded_len(&self) -> u64 {
        self.children.encoded_len()
    }
}

impl ParseBox for MoovBox {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        let children = Boxes::parse(buf)?;
        Ok(Self { children })
    }

    fn box_type() -> BoxType {
        NAME
    }
}
impl ParsedBox for MoovBox {
    fn encoded_len(&self) -> u64 {
        self.children.encoded_len()
    }

    fn put_buf(&self, mut out: &mut dyn BufMut) {
        self.children.put_buf(&mut out);
    }
}
