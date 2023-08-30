#![allow(missing_docs)]

use derive_more::{Deref, DerefMut};
use nonempty::NonEmpty;

use crate::error::Result;

use super::{Boxes, ParseBox, ParseBoxes, ParseError, ParsedBox, TrakBox};

#[derive(Clone, Debug, Deref, DerefMut, ParseBox, ParsedBox)]
#[box_type = "moov"]
pub struct MoovBox {
    pub children: Boxes<MoovChildren>,
}

#[non_exhaustive]
#[derive(Clone, Debug, ParseBoxes)]
#[box_type = "moov"]
pub struct MoovChildren {
    pub tracks: NonEmpty<TrakBox>,
}

impl MoovBox {
    #[cfg(test)]
    pub fn new(tracks: NonEmpty<TrakBox>) -> Result<Self, ParseError> {
        Self::with_children(MoovChildren { tracks })
    }

    pub fn with_children(children: MoovChildren) -> Result<Self, ParseError> {
        Ok(Self { children: Boxes::new(children, [])? })
    }
}

#[cfg(test)]
mod test {
    use bytes::BytesMut;

    use crate::parse::{BoxType, MdiaBox, MinfBox, StblBox, StblCo, StcoBox};

    use super::*;

    #[test]
    fn roundtrip() {
        let mut data = BytesMut::new();
        let test_moov = || {
            let co = StblCo::Stco(StcoBox::from_iter([]));
            let track = TrakBox::new(MdiaBox::new(MinfBox::new(StblBox::new(co)?)?)?)?;
            MoovBox::new(NonEmpty::new(track))
        };
        test_moov().unwrap().put_buf(&mut data);
        MoovBox::parse(&mut data).unwrap();
    }

    #[test]
    fn no_traks() {
        const NO_TRAKS_MOOV: &[&[u8]] = &[
            &[0, 0, 0, 16], // box size
            b"moov",        // box type
            //
            // mvhd box (inside moov box)
            //
            &[0, 0, 0, 8],
            b"mvhd",
        ];

        let err = MoovBox::parse(&mut NO_TRAKS_MOOV.concat()[..].into()).unwrap_err();
        assert!(
            matches!(err.get_ref(), ParseError::MissingRequiredBox(BoxType::TRAK)),
            "{err}",
        );
    }
}
