use bytes::{BufMut, BytesMut};
use error_stack::Result;

use super::error::{ParseResultExt, WhileParsingChild};
use super::mp4box::{Boxes, ParseBox};
use super::{BoxType, Co64Box, ParseError, ParsedBox, StcoBox};

#[derive(Clone, Debug)]
pub struct StblBox {
    children: Boxes,
}

#[derive(Debug)]
pub enum StblCoMut<'a> {
    Stco(&'a mut StcoBox),
    Co64(&'a mut Co64Box),
}

const NAME: BoxType = BoxType::STBL;
const STCO: BoxType = BoxType::STCO;
const CO64: BoxType = BoxType::CO64;

impl StblBox {
    pub fn co_mut(&mut self) -> Result<StblCoMut<'_>, ParseError> {
        let have_stco = self.children.box_types().any(|box_type| box_type == STCO);
        let have_co64 = self.children.box_types().any(|box_type| box_type == CO64);
        ensure_attach!(
            !(have_stco && have_co64),
            ParseError::InvalidBoxLayout,
            "more than one stco and co64 present",
            WhileParsingChild(NAME, STCO),
        );
        if have_stco {
            self.children
                .get_one_mut()
                .while_parsing_child(NAME, STCO)
                .map(StblCoMut::Stco)
        } else {
            self.children
                .get_one_mut()
                .while_parsing_child(NAME, CO64)
                .map(StblCoMut::Co64)
        }
    }
}

impl ParseBox for StblBox {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        let children = Boxes::parse(buf).while_parsing_field(NAME, "children")?;
        Ok(Self { children })
    }

    fn box_type() -> BoxType {
        NAME
    }
}

impl ParsedBox for StblBox {
    fn encoded_len(&self) -> u64 {
        self.children.encoded_len()
    }

    fn put_buf(&self, buf: &mut dyn BufMut) {
        self.children.put_buf(buf)
    }
}
