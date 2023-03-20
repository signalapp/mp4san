#![allow(missing_docs)]

use bytes::{BufMut, BytesMut};
use error_stack::Result;

use super::error::{ParseResultExt, WhileParsingField};
use super::mp4box::Boxes;
use super::{BoxType, ParseBox, ParseError, ParsedBox, TrakBox};

#[derive(Clone, Debug)]
pub struct MoovBox {
    children: Boxes,
}

const NAME: BoxType = BoxType::MOOV;

impl MoovBox {
    #[cfg(test)]
    pub(crate) fn with_children<C: Into<Boxes>>(children: C) -> Self {
        Self { children: children.into() }
    }

    pub fn traks(&mut self) -> impl Iterator<Item = Result<&mut TrakBox, ParseError>> + '_ {
        self.children
            .get_mut()
            .map(|result| result.while_parsing_child(NAME, BoxType::TRAK))
    }

    pub fn encoded_len(&self) -> u64 {
        self.children.encoded_len()
    }
}

impl ParseBox for MoovBox {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        let children = Boxes::parse(buf).while_parsing_field(NAME, "children")?;
        ensure_attach!(
            children.box_types().any(|box_type| box_type == BoxType::TRAK),
            ParseError::MissingRequiredBox(BoxType::TRAK),
            WhileParsingField(NAME, "children"),
        );
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

#[cfg(test)]
mod test {
    use crate::parse::Mp4Box;

    use super::*;

    fn test_trak() -> Mp4Box<TrakBox> {
        Mp4Box::with_data(TrakBox::with_children(vec![]).into()).unwrap()
    }

    #[test]
    fn roundtrip() {
        let mut data = BytesMut::new();
        MoovBox::with_children(vec![test_trak().into()]).put_buf(&mut data);
        MoovBox::parse(&mut data).unwrap();
    }

    #[test]
    fn no_traks() {
        let mut data = BytesMut::new();
        MoovBox::with_children(vec![]).put_buf(&mut data);
        let err = MoovBox::parse(&mut data).unwrap_err();
        assert!(
            matches!(err.current_context(), ParseError::MissingRequiredBox(BoxType::TRAK)),
            "{err}",
        );
    }
}
