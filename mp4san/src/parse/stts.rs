#![allow(missing_docs)]

use super::{ArrayEntryMut, BoundedArray, ConstFullBoxHeader, Mp4Prim, ParseBox, ParsedBox};

#[derive(Clone, Debug, Default, ParseBox, ParsedBox)]
#[box_type = "stts"]
pub struct SttsBox {
    _parsed_header: ConstFullBoxHeader,
    entries: BoundedArray<u32, SttsEntry>,
}

#[derive(Clone, Copy, Debug, Mp4Prim)]
pub struct SttsEntry {
    pub sample_count: u32,
    pub sample_delta: u32,
}

impl SttsBox {
    pub fn entries_mut(&mut self) -> impl ExactSizeIterator<Item = ArrayEntryMut<'_, SttsEntry>> + '_ {
        self.entries.entries_mut()
    }

    pub fn entry_count(&self) -> u32 {
        self.entries.entry_count()
    }
}

impl FromIterator<SttsEntry> for SttsBox {
    fn from_iter<I: IntoIterator<Item = SttsEntry>>(entries: I) -> Self {
        Self { _parsed_header: Default::default(), entries: entries.into_iter().collect() }
    }
}

#[cfg(test)]
mod test {
    use bytes::BytesMut;

    use crate::parse::{ParseBox, ParsedBox};

    use super::SttsBox;

    #[test]
    fn roundtrip() {
        let mut buf = BytesMut::new();
        SttsBox::default().put_buf(&mut buf);
        SttsBox::parse(&mut buf).unwrap();
    }
}
