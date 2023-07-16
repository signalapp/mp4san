#![allow(missing_docs)]

use crate::error::Result;

use super::error::{ParseResultExt, WhileParsingField};
use super::{BoxType, Boxes, BoxesValidator, ParseBox, ParseError, ParsedBox, TrakBox};

#[derive(Clone, Debug, ParseBox, ParsedBox)]
#[box_type = "moov"]
pub struct MoovBox {
    children: Boxes<MoovChildrenValidator>,
}

pub(crate) struct MoovChildrenValidator;

const NAME: BoxType = BoxType::MOOV;

impl MoovBox {
    #[cfg(test)]
    pub(crate) fn with_children<C: Into<Boxes<MoovChildrenValidator>>>(children: C) -> Self {
        Self { children: children.into() }
    }

    pub fn traks(&mut self) -> impl Iterator<Item = Result<&mut TrakBox, ParseError>> + '_ {
        self.children
            .get_mut()
            .map(|result| result.while_parsing_child(NAME, BoxType::TRAK))
    }
}

impl BoxesValidator for MoovChildrenValidator {
    fn validate<V>(children: &Boxes<V>) -> Result<(), ParseError> {
        ensure_attach!(
            children.box_types().any(|box_type| box_type == BoxType::TRAK),
            ParseError::MissingRequiredBox(BoxType::TRAK),
            WhileParsingField(NAME, "children"),
        );
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use bytes::BytesMut;

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
            matches!(err.get_ref(), ParseError::MissingRequiredBox(BoxType::TRAK)),
            "{err}",
        );
    }
}
