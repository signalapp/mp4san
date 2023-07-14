#![allow(missing_docs)]

use super::{ArrayEntryMut, BoundedArray, ConstFullBoxHeader, ParseBox, ParseError, ParsedBox};

#[derive(Clone, Debug, Default, ParseBox, ParsedBox)]
#[box_type = "co64"]
pub struct Co64Box {
    header: ConstFullBoxHeader,
    entries: BoundedArray<u32, u64>,
}

impl Co64Box {
    #[cfg(test)]
    pub(crate) fn with_entries<I: IntoIterator<Item = u64>>(entries: I) -> Self {
        Self { header: Default::default(), entries: BoundedArray::with_entries(entries) }
    }

    pub fn entries_mut(&mut self) -> impl Iterator<Item = ArrayEntryMut<'_, u64>> + ExactSizeIterator + '_ {
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

    use super::Co64Box;

    #[test]
    fn roundtrip() {
        let mut buf = BytesMut::new();
        Co64Box::default().put_buf(&mut buf);
        Co64Box::parse(&mut buf).unwrap();
    }
}
