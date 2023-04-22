use std::fmt::Debug;
use std::marker::PhantomData;
use std::mem::{size_of, take};

use bytes::{BufMut, BytesMut};
use error_stack::Result;

use super::error::WhileParsingField;
use super::{BoxType, FullBoxHeader, Mpeg4Int, Mpeg4IntWriterExt, ParseError, ParsedBox};

#[derive(Clone, Debug, Default)]
pub struct CoBox<T> {
    entries: BytesMut,
    _t: PhantomData<T>,
}

pub struct CoEntry<'a, T> {
    data: &'a mut [u8],
    _t: PhantomData<T>,
}

impl<T> CoBox<T> {
    const FULL_BOX_HEADER: FullBoxHeader = FullBoxHeader::default();

    #[cfg(test)]
    pub(crate) fn with_entries<I: IntoIterator<Item = T>>(entries: I) -> Self
    where
        T: Mpeg4Int,
    {
        let mut entries_bytes = BytesMut::new();
        for entry in entries {
            entry.put_buf(&mut entries_bytes);
        }
        Self { entries: entries_bytes, _t: PhantomData }
    }

    pub fn parse(mut buf: &mut BytesMut, name: BoxType) -> Result<Self, ParseError> {
        FullBoxHeader::parse(&mut buf)?.ensure_eq(&Self::FULL_BOX_HEADER)?;

        let entry_count = u32::parse(&mut buf)?;
        let entries_len = size_of::<T>().checked_mul(entry_count as usize).ok_or_else(|| {
            report_attach!(
                ParseError::InvalidInput,
                "overflow",
                WhileParsingField(name, "entry_count"),
            )
        })?;
        ensure_attach!(
            entries_len >= buf.len(),
            ParseError::InvalidInput,
            "extra unparsed data",
            WhileParsingField(name, "entries"),
        );
        ensure_attach!(
            entries_len <= buf.len(),
            ParseError::TruncatedBox,
            WhileParsingField(name, "entries"),
        );
        let entries = take(buf);
        Ok(Self { entries, _t: PhantomData })
    }

    pub fn entries_mut(&mut self) -> impl Iterator<Item = CoEntry<'_, T>> + ExactSizeIterator + '_ {
        self.entries
            .chunks_exact_mut(size_of::<T>())
            .map(|data| CoEntry { data, _t: PhantomData })
    }

    pub fn entry_count(&self) -> u32 {
        (self.entries.len() / size_of::<T>()) as u32
    }
}

impl<T: Clone + Debug + 'static> ParsedBox for CoBox<T> {
    fn encoded_len(&self) -> u64 {
        Self::FULL_BOX_HEADER.encoded_len() + size_of::<u32>() as u64 + self.entries.len() as u64
    }

    fn put_buf(&self, buf: &mut dyn BufMut) {
        Self::FULL_BOX_HEADER.put_buf(&mut *buf);
        buf.put_u32((self.entry_count()).try_into().unwrap());
        buf.put_slice(&self.entries[..])
    }
}

impl<T: Mpeg4Int> CoEntry<'_, T> {
    pub fn get(&self) -> T {
        T::parse(&*self.data).unwrap_or_else(|_| unreachable!())
    }

    pub fn set(&mut self, value: T) {
        self.data.put_mp4int(value)
    }
}
