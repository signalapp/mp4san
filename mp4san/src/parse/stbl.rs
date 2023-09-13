#![allow(missing_docs)]

use std::vec;

use derive_more::{Deref, DerefMut};

use crate::{error::Result, parse::error::WhileParsingBox};

use super::{AnyMp4Box, Boxes, Co64Box, ParseBox, ParseBoxes, ParseError, ParsedBox, StcoBox};

#[derive(Clone, Debug, Deref, DerefMut, ParseBox, ParsedBox)]
#[box_type = "stbl"]
pub struct StblBox {
    pub children: Boxes<StblChildren>,
}

#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct StblChildren {
    pub chunk_offsets: StblCo,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct StblChildrenRef<'a> {
    pub chunk_offsets: StblCoRef<'a>,
}

#[non_exhaustive]
#[derive(Debug)]
pub struct StblChildrenRefMut<'a> {
    pub chunk_offsets: StblCoRefMut<'a>,
}

#[derive(Clone, Debug)]
pub enum StblCo {
    Stco(StcoBox),
    Co64(Co64Box),
}

#[derive(Clone, Copy, Debug)]
pub enum StblCoRef<'a> {
    Stco(&'a StcoBox),
    Co64(&'a Co64Box),
}

#[derive(Debug)]
pub enum StblCoRefMut<'a> {
    Stco(&'a mut StcoBox),
    Co64(&'a mut Co64Box),
}

#[non_exhaustive]
#[derive(Clone, Debug, ParseBoxes)]
#[box_type = "stbl"]
struct DerivedStblChildren {
    chunk_offsets_32: Option<StcoBox>,
    chunk_offsets_64: Option<Co64Box>,
}

impl StblBox {
    pub fn with_children(children: StblChildren) -> Result<Self, ParseError> {
        Ok(Self { children: Boxes::new(children, [])? })
    }
}

//
// StblCo impls
//

impl ParseBoxes for StblChildren {
    type Ref<'a> = StblChildrenRef<'a>;
    type RefMut<'a> = StblChildrenRefMut<'a>;
    type IntoIter = vec::IntoIter<AnyMp4Box>;

    fn parse<'a>(boxes: &'a mut [AnyMp4Box]) -> Result<Self::RefMut<'a>, ParseError>
    where
        Self: 'a,
    {
        let derived = DerivedStblChildren::parse(boxes)?;
        let chunk_offsets = match (derived.chunk_offsets_32, derived.chunk_offsets_64) {
            (Some(chunk_offsets_32), None) => StblCoRefMut::Stco(chunk_offsets_32),
            (None, Some(chunk_offsets_64)) => StblCoRefMut::Co64(chunk_offsets_64),
            (Some(_), Some(_)) => bail_attach!(
                ParseError::InvalidBoxLayout,
                "more than one stco and co64 present",
                WhileParsingBox(StblBox::NAME),
            ),
            (None, None) => bail_attach!(
                ParseError::MissingRequiredBox(StcoBox::NAME),
                WhileParsingBox(StblBox::NAME),
            ),
        };
        Ok(Self::RefMut { chunk_offsets })
    }

    fn parsed<'a>(boxes: &'a [AnyMp4Box]) -> Self::Ref<'a>
    where
        Self: 'a,
    {
        let DerivedStblChildrenRef { chunk_offsets_32, chunk_offsets_64 } = DerivedStblChildren::parsed(boxes);
        let chunk_offsets_32 = chunk_offsets_32.map(StblCoRef::Stco);
        let chunk_offsets_64 = chunk_offsets_64.map(StblCoRef::Co64);
        let chunk_offsets = chunk_offsets_32.or(chunk_offsets_64).unwrap();
        Self::Ref { chunk_offsets }
    }

    fn try_into_iter(self) -> Result<Self::IntoIter, ParseError> {
        let (stco, co64) = match self.chunk_offsets {
            StblCo::Stco(stco) => (Some(stco), None),
            StblCo::Co64(co64) => (None, Some(co64)),
        };
        DerivedStblChildren { chunk_offsets_32: stco, chunk_offsets_64: co64 }.try_into_iter()
    }
}

//
// StblCo impls
//

impl Default for StblCo {
    fn default() -> Self {
        Self::Stco(Default::default())
    }
}

//
// StblCoRef impls
//

impl StblCoRef<'_> {
    pub fn entry_count(&self) -> u32 {
        match self {
            Self::Stco(stco) => stco.entry_count(),
            Self::Co64(co64) => co64.entry_count(),
        }
    }
}

//
// StblCoMut impls
//

impl StblCoRefMut<'_> {
    pub fn entry_count(&self) -> u32 {
        match self {
            Self::Stco(stco) => stco.entry_count(),
            Self::Co64(co64) => co64.entry_count(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl StblBox {
        pub(crate) fn dummy() -> Self {
            Self::new(Default::default()).unwrap()
        }

        #[cfg(test)]
        pub(crate) fn new(chunk_offsets: StblCo) -> Result<Self, ParseError> {
            Self::with_children(StblChildren { chunk_offsets })
        }
    }
}
