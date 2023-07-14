#![allow(missing_docs)]

use bytes::{BufMut, BytesMut};

use crate::error::Result;

use super::co::CoBox;
use super::{ArrayEntryMut, BoxType, ParseBox, ParseError, ParsedBox};

#[derive(Clone, Debug, Default)]
pub struct Co64Box {
    inner: CoBox<u64>,
}

const NAME: BoxType = BoxType::CO64;

impl Co64Box {
    #[cfg(test)]
    pub(crate) fn with_entries<I: IntoIterator<Item = u64>>(entries: I) -> Self {
        Self { inner: CoBox::with_entries(entries) }
    }

    pub fn entries_mut(&mut self) -> impl Iterator<Item = ArrayEntryMut<'_, u64>> + ExactSizeIterator + '_ {
        self.inner.entries_mut()
    }

    pub fn entry_count(&self) -> u32 {
        self.inner.entry_count()
    }
}

impl ParseBox for Co64Box {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        Ok(Self { inner: CoBox::parse(buf, NAME)? })
    }

    fn box_type() -> BoxType {
        NAME
    }
}

impl ParsedBox for Co64Box {
    fn encoded_len(&self) -> u64 {
        self.inner.encoded_len()
    }

    fn put_buf(&self, buf: &mut dyn BufMut) {
        self.inner.put_buf(buf)
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
        dbg!(&mut buf);
        Co64Box::parse(&mut buf).unwrap();
    }
}
