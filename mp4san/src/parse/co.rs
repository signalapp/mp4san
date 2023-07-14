use bytes::BytesMut;
use derive_where::derive_where;

use crate::error::Result;

use super::error::{ParseResultExt, WhileParsingField};
use super::{ArrayEntryMut, BoundedArray, BoxType, FullBoxHeader, Mp4Prim, Mp4ValueReaderExt, ParseError, ParsedBox};

#[derive(Default, ParsedBox)]
#[derive_where(Clone, Debug)]
pub struct CoBox<T: Mp4Prim + 'static> {
    header: FullBoxHeader,
    entries: BoundedArray<u32, T>,
}

impl<T: Mp4Prim> CoBox<T> {
    const FULL_BOX_HEADER: FullBoxHeader = FullBoxHeader::default();

    #[cfg(test)]
    pub(crate) fn with_entries<I: IntoIterator<Item = T>>(entries: I) -> Self
    where
        T: Mp4Prim,
    {
        Self { header: Self::FULL_BOX_HEADER, entries: BoundedArray::with_entries(entries) }
    }

    pub fn parse(buf: &mut BytesMut, name: BoxType) -> Result<Self, ParseError> {
        let header = <FullBoxHeader as Mp4Prim>::parse(&mut *buf)?;
        header.ensure_eq(&Self::FULL_BOX_HEADER)?;
        let entries = buf.get_mp4_value().while_parsing_field(name, "entries")?;

        ensure_attach!(
            buf.is_empty(),
            ParseError::InvalidInput,
            "extra unparsed data",
            WhileParsingField(name, "entries"),
        );
        Ok(Self { header, entries })
    }

    pub fn entries_mut(&mut self) -> impl Iterator<Item = ArrayEntryMut<'_, T>> + ExactSizeIterator + '_ {
        self.entries.entries_mut()
    }

    pub fn entry_count(&self) -> u32 {
        self.entries.entry_count()
    }
}
