#![allow(missing_docs)]

use super::{ArrayEntryMut, BoundedArray, ConstFullBoxHeader, ParseBox, ParseError, ParsedBox};

#[derive(Clone, Debug, Default, ParseBox, ParsedBox)]
#[box_type = "co64"]
pub struct Co64Box {
    header: ConstFullBoxHeader,
    entries: BoundedArray<u32, u64>,
}

impl Co64Box {
    pub fn entries_mut(&mut self) -> impl Iterator<Item = ArrayEntryMut<'_, u64>> + ExactSizeIterator + '_ {
        self.entries.entries_mut()
    }

    pub fn entry_count(&self) -> u32 {
        self.entries.entry_count()
    }
}

impl FromIterator<u64> for Co64Box {
    fn from_iter<I: IntoIterator<Item = u64>>(entries: I) -> Self {
        Self { header: Default::default(), entries: entries.into_iter().collect() }
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
