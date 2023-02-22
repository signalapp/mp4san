use std::mem::{size_of, take};

use bytes::{BufMut, BytesMut};
use error_stack::Result;

use super::error::WhileParsingField;
use super::{BoxType, Mpeg4Int, ParseBox, ParseError, ParsedBox};

#[derive(Clone, Debug)]
pub struct Co64Box {
    entries: BytesMut,
}

pub struct Co64Entry<'a> {
    data: &'a mut [u8; size_of::<u64>()],
}

const NAME: BoxType = BoxType::CO64;

impl Co64Box {
    pub fn entries_mut(&mut self) -> impl Iterator<Item = Co64Entry<'_>> + ExactSizeIterator + '_ {
        self.entries
            .chunks_exact_mut(size_of::<u64>())
            .map(|data| Co64Entry { data: data.try_into().unwrap_or_else(|_| unreachable!()) })
    }
}

impl ParseBox for Co64Box {
    fn parse(mut buf: &mut BytesMut) -> Result<Self, ParseError> {
        let entry_count = u32::parse(&mut buf)?;
        let entries_len = size_of::<u64>().checked_mul(entry_count as usize).ok_or_else(|| {
            report_attach!(
                ParseError::InvalidInput,
                "overflow",
                WhileParsingField(NAME, "entry_count"),
            )
        })?;
        ensure_attach!(
            entries_len >= buf.len(),
            ParseError::InvalidInput,
            "extra unparsed data",
            WhileParsingField(NAME, "entries"),
        );
        ensure_attach!(
            entries_len <= buf.len(),
            ParseError::TruncatedBox,
            WhileParsingField(NAME, "entries"),
        );
        let entries = take(buf);
        Ok(Self { entries })
    }

    fn box_type() -> BoxType {
        NAME
    }
}

impl ParsedBox for Co64Box {
    fn encoded_len(&self) -> u64 {
        self.entries.len() as u64
    }

    fn put_buf(&self, buf: &mut dyn BufMut) {
        buf.put_slice(&self.entries[..])
    }
}

impl Co64Entry<'_> {
    pub fn get(&self) -> u64 {
        u64::from_be_bytes(*self.data)
    }

    pub fn set(&mut self, value: u64) {
        *self.data = value.to_be_bytes();
    }
}
