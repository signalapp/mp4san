#![allow(missing_docs)]

use super::{ArrayEntryMut, BoundedArray, ConstFullBoxHeader, ParseBox, ParseError, ParsedBox};

#[derive(Clone, Debug, Default, ParseBox, ParsedBox)]
#[box_type = "stco"]
pub struct StcoBox {
    header: ConstFullBoxHeader,
    entries: BoundedArray<u32, u32>,
}

impl StcoBox {
    #[cfg(test)]
    pub(crate) fn with_entries<I: IntoIterator<Item = u32>>(entries: I) -> Self {
        Self { header: Default::default(), entries: BoundedArray::with_entries(entries) }
    }

    pub fn entries_mut(&mut self) -> impl Iterator<Item = ArrayEntryMut<'_, u32>> + ExactSizeIterator + '_ {
        self.entries.entries_mut()
    }

    pub fn entry_count(&self) -> u32 {
        self.entries.entry_count()
    }
}

#[cfg(test)]
mod test {
    use bytes::BytesMut;

    use crate::parse::{ParseBox, ParsedBox};

    use super::StcoBox;

    #[test]
    fn roundtrip() {
        let mut buf = BytesMut::new();
        StcoBox::default().put_buf(&mut buf);
        StcoBox::parse(&mut buf).unwrap();
    }
}
