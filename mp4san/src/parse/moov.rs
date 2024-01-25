#![allow(missing_docs)]

use derive_more::{Deref, DerefMut};
use nonempty::NonEmpty;

use crate::error::Result;

use super::{Boxes, MvhdBox, ParseBox, ParseBoxes, ParseError, ParsedBox, TrakBox};

#[derive(Clone, Debug, Deref, DerefMut, ParseBox, ParsedBox)]
#[box_type = "moov"]
pub struct MoovBox {
    pub children: Boxes<MoovChildren>,
}

#[non_exhaustive]
#[derive(Clone, Debug, ParseBoxes)]
#[box_type = "moov"]
pub struct MoovChildren {
    pub header: MvhdBox,
    pub tracks: NonEmpty<TrakBox>,
}

impl MoovBox {
    pub fn with_children(children: MoovChildren) -> Result<Self, ParseError> {
        Ok(Self { children: Boxes::new(children, [])? })
    }
}

#[cfg(test)]
mod test {
    use crate::parse::{BoxType, Mp4Value};
    use crate::util::test::test_mvhd;

    use super::*;

    impl MoovBox {
        pub(crate) fn dummy() -> Self {
            Self::new(MvhdBox::dummy(), NonEmpty::new(TrakBox::dummy())).unwrap()
        }

        pub(crate) fn new(header: MvhdBox, tracks: NonEmpty<TrakBox>) -> Result<Self, ParseError> {
            Self::with_children(MoovChildren { header, tracks })
        }
    }

    #[test]
    fn roundtrip() {
        MoovBox::parse(&mut MoovBox::dummy().to_bytes()).unwrap();
    }

    #[test]
    fn no_traks() {
        let err = MoovBox::parse(&mut test_mvhd().to_bytes()).unwrap_err();
        assert!(
            matches!(err.get_ref(), ParseError::MissingRequiredBox(BoxType::TRAK)),
            "{err}",
        );
    }
}
