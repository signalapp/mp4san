use std::convert::TryInto;
use std::mem::{size_of, take};

use bytes::{BufMut, BytesMut};

use super::mp4box::ParseBox;
use super::{BoxType, Mpeg4Int, ParseError, ParsedBox};

#[derive(Clone, Debug)]
pub struct StcoBox {
    entries: BytesMut,
}

pub struct StcoEntry<'a> {
    data: &'a mut [u8; size_of::<u32>()],
}

const NAME: BoxType = BoxType::STCO;

impl StcoBox {
    pub fn entries_mut(&mut self) -> impl Iterator<Item = StcoEntry<'_>> + ExactSizeIterator + '_ {
        self.entries
            .chunks_exact_mut(size_of::<u32>())
            .map(|data| StcoEntry { data: data.try_into().unwrap_or_else(|_| unreachable!()) })
    }
}

impl ParseBox for StcoBox {
    fn parse(mut buf: &mut BytesMut) -> Result<Self, ParseError> {
        let entry_count = u32::parse(&mut buf)?;
        let entries_len = size_of::<u32>()
            .checked_mul(entry_count as usize)
            .ok_or(ParseError::InvalidInput("stco entry count overflow"))?;
        if entries_len < buf.len() {
            return Err(ParseError::InvalidInput("extra unparsed stco data"));
        }
        if entries_len > buf.len() {
            return Err(ParseError::TruncatedBox);
        }
        let entries = take(buf);
        Ok(Self { entries })
    }

    fn box_type() -> BoxType {
        NAME
    }
}

impl ParsedBox for StcoBox {
    fn encoded_len(&self) -> u64 {
        self.entries.len() as u64
    }

    fn put_buf(&self, buf: &mut dyn BufMut) {
        buf.put_slice(&self.entries[..])
    }
}

impl StcoEntry<'_> {
    pub fn get(&self) -> u32 {
        u32::from_be_bytes(*self.data)
    }

    pub fn set(&mut self, value: u32) {
        *self.data = value.to_be_bytes();
    }
}
