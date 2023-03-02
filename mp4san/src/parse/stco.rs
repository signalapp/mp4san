use bytes::{BufMut, BytesMut};
use error_stack::Result;

use super::co::{CoBox, CoEntry};
use super::{BoxType, ParseBox, ParseError, ParsedBox};

#[derive(Clone, Debug, Default)]
pub struct StcoBox {
    inner: CoBox<u32>,
}

pub struct StcoEntry<'a> {
    inner: CoEntry<'a, u32>,
}

const NAME: BoxType = BoxType::STCO;

impl StcoBox {
    pub fn entries_mut(&mut self) -> impl Iterator<Item = StcoEntry<'_>> + ExactSizeIterator + '_ {
        self.inner.entries_mut().map(|inner| StcoEntry { inner })
    }
}

impl ParseBox for StcoBox {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        Ok(Self { inner: CoBox::parse(buf, NAME)? })
    }

    fn box_type() -> BoxType {
        NAME
    }
}

impl ParsedBox for StcoBox {
    fn encoded_len(&self) -> u64 {
        self.inner.encoded_len()
    }

    fn put_buf(&self, buf: &mut dyn BufMut) {
        self.inner.put_buf(buf)
    }
}

impl StcoEntry<'_> {
    pub fn get(&self) -> u32 {
        self.inner.get()
    }

    pub fn set(&mut self, value: u32) {
        self.inner.set(value)
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
