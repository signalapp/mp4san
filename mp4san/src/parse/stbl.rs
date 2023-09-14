#![allow(missing_docs)]

use std::vec;

use derive_more::{Deref, DerefMut};

use crate::error::Result;
use crate::parse::error::WhileParsingBox;

use super::{
    AnyMp4Box, Boxes, Co64Box, ParseBox, ParseBoxes, ParseError, ParsedBox, StcoBox, StscBox, StsdBox, SttsBox,
};

#[derive(Clone, Debug, Deref, DerefMut, ParseBox, ParsedBox)]
#[box_type = "stbl"]
pub struct StblBox {
    pub children: Boxes<StblChildren>,
}

#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct StblChildren {
    pub sample_descriptions: StsdBox,
    pub time_to_sample: SttsBox,
    pub sample_to_chunk: StscBox,
    pub chunk_offsets: StblCo,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct StblChildrenRef<'a> {
    pub sample_descriptions: &'a StsdBox,
    pub time_to_sample: &'a SttsBox,
    pub sample_to_chunk: &'a StscBox,
    pub chunk_offsets: StblCoRef<'a>,
}

#[non_exhaustive]
#[derive(Debug)]
pub struct StblChildrenRefMut<'a> {
    pub sample_descriptions: &'a mut StsdBox,
    pub time_to_sample: &'a mut SttsBox,
    pub sample_to_chunk: &'a mut StscBox,
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
    sample_descriptions: StsdBox,
    time_to_sample: SttsBox,
    sample_to_chunk: StscBox,
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
        let DerivedStblChildrenRefMut {
            sample_descriptions,
            time_to_sample,
            sample_to_chunk,
            chunk_offsets_32,
            chunk_offsets_64,
        } = DerivedStblChildren::parse(boxes)?;
        let chunk_offsets = match (chunk_offsets_32, chunk_offsets_64) {
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
        Ok(Self::RefMut { sample_descriptions, time_to_sample, sample_to_chunk, chunk_offsets })
    }

    fn parsed<'a>(boxes: &'a [AnyMp4Box]) -> Self::Ref<'a>
    where
        Self: 'a,
    {
        let DerivedStblChildrenRef {
            sample_descriptions,
            time_to_sample,
            sample_to_chunk,
            chunk_offsets_32,
            chunk_offsets_64,
        } = DerivedStblChildren::parsed(boxes);
        let chunk_offsets_32 = chunk_offsets_32.map(StblCoRef::Stco);
        let chunk_offsets_64 = chunk_offsets_64.map(StblCoRef::Co64);
        let chunk_offsets = chunk_offsets_32.or(chunk_offsets_64).unwrap();
        Self::Ref { sample_descriptions, time_to_sample, sample_to_chunk, chunk_offsets }
    }

    fn try_into_iter(self) -> Result<Self::IntoIter, ParseError> {
        let Self { sample_descriptions, time_to_sample, sample_to_chunk, chunk_offsets } = self;
        let (chunk_offsets_32, chunk_offsets_64) = match chunk_offsets {
            StblCo::Stco(chunk_offsets_32) => (Some(chunk_offsets_32), None),
            StblCo::Co64(chunk_offsets_64) => (None, Some(chunk_offsets_64)),
        };
        DerivedStblChildren { sample_descriptions, time_to_sample, sample_to_chunk, chunk_offsets_32, chunk_offsets_64 }
            .try_into_iter()
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
            Self::new(
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .unwrap()
        }

        #[cfg(test)]
        pub(crate) fn new(
            sample_descriptions: StsdBox,
            time_to_sample: SttsBox,
            sample_to_chunk: StscBox,
            chunk_offsets: StblCo,
        ) -> Result<Self, ParseError> {
            Self::with_children(StblChildren { sample_descriptions, time_to_sample, sample_to_chunk, chunk_offsets })
        }
    }
}
