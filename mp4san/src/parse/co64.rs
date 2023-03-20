#![allow(missing_docs)]

use bytes::{BufMut, BytesMut};
use error_stack::Result;

use super::co::{CoBox, CoEntry};
use super::{BoxType, ParseBox, ParseError, ParsedBox};

#[derive(Clone, Debug, Default)]
pub struct Co64Box {
    inner: CoBox<u64>,
}

pub struct Co64Entry<'a> {
    inner: CoEntry<'a, u64>,
}

const NAME: BoxType = BoxType::CO64;

impl Co64Box {
    #[cfg(test)]
    pub(crate) fn with_entries<I: IntoIterator<Item = u64>>(entries: I) -> Self {
        Self { inner: CoBox::with_entries(entries) }
    }

    pub fn entries_mut(&mut self) -> impl Iterator<Item = Co64Entry<'_>> + ExactSizeIterator + '_ {
        self.inner.entries_mut().map(|inner| Co64Entry { inner })
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

impl Co64Entry<'_> {
    pub fn get(&self) -> u64 {
        self.inner.get()
    }

    pub fn set(&mut self, value: u64) {
        self.inner.set(value)
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
