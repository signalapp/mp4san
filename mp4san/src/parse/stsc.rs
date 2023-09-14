#![allow(missing_docs)]

use super::{ArrayEntryMut, BoundedArray, ConstFullBoxHeader, Mp4Prim, ParseBox, ParsedBox};

#[derive(Clone, Debug, Default, ParseBox, ParsedBox)]
#[box_type = "stsc"]
pub struct StscBox {
    _parsed_header: ConstFullBoxHeader,
    entries: BoundedArray<u32, StscEntry>,
}

#[derive(Clone, Copy, Debug, Mp4Prim)]
pub struct StscEntry {
    pub first_chunk: u32,
    pub samples_per_chunk: u32,
    pub samples_description_index: u32,
}

#[derive(Mp4Prim)]
pub enum Test {
    A([u8; 4]),
    B(u32),
}

impl StscBox {
    pub fn entries_mut(&mut self) -> impl ExactSizeIterator<Item = ArrayEntryMut<'_, StscEntry>> + '_ {
        self.entries.entries_mut()
    }

    pub fn entry_count(&self) -> u32 {
        self.entries.entry_count()
    }
}

impl FromIterator<StscEntry> for StscBox {
    fn from_iter<I: IntoIterator<Item = StscEntry>>(entries: I) -> Self {
        Self { _parsed_header: Default::default(), entries: entries.into_iter().collect() }
    }
}

#[cfg(test)]
mod test {
    use bytes::BytesMut;

    use crate::parse::{ParseBox, ParsedBox};

    use super::StscBox;

    #[test]
    fn roundtrip() {
        let mut buf = BytesMut::new();
        StscBox::default().put_buf(&mut buf);
        StscBox::parse(&mut buf).unwrap();
    }
}
