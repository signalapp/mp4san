#![allow(missing_docs)]

use std::mem::size_of;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use error_stack::Result;

use super::error::{ParseResultExt, WhereEq, WhileParsingField};
use super::mp4box::ParsedBox;
use super::{BoxType, FourCC, Mpeg4IntReaderExt, ParseBox, ParseError};

#[derive(Clone, Debug)]
pub struct FtypBox {
    pub major_brand: FourCC,
    pub minor_version: u32,
    compatible_brands: Bytes,
}

const NAME: BoxType = BoxType::FTYP;

impl FtypBox {
    pub fn new(major_brand: FourCC, minor_version: u32, compatible_brands: impl IntoIterator<Item = FourCC>) -> Self {
        let compatible_brands = compatible_brands.into_iter().flat_map(|fourcc| fourcc.value).collect();
        Self { major_brand, minor_version, compatible_brands }
    }
    pub fn compatible_brands(&self) -> impl Iterator<Item = FourCC> + ExactSizeIterator + '_ {
        self.compatible_brands
            .chunks_exact(4)
            .map(|bytes| FourCC { value: bytes.try_into().unwrap() })
    }
}

impl ParseBox for FtypBox {
    fn parse(reader: &mut BytesMut) -> Result<Self, ParseError> {
        let major_brand = reader.get().while_parsing_field(NAME, "major_brand")?;
        let minor_version = reader.get().while_parsing_field(NAME, "minor_version")?;

        ensure_attach!(
            reader.remaining() % FourCC::size() as usize == 0,
            ParseError::TruncatedBox,
            WhileParsingField(NAME, "compatible_brands"),
            WhereEq("reader.remaining()", reader.remaining()),
        );

        let compatible_brands = reader.copy_to_bytes(reader.remaining());

        Ok(Self { major_brand, minor_version, compatible_brands })
    }

    fn box_type() -> BoxType {
        NAME
    }
}

impl ParsedBox for FtypBox {
    fn encoded_len(&self) -> u64 {
        FourCC::size() + size_of::<u32>() as u64 + self.compatible_brands.len() as u64
    }

    fn put_buf(&self, mut out: &mut dyn BufMut) {
        self.major_brand.put_buf(&mut out);
        out.put_u32(self.minor_version);
        out.put_slice(&self.compatible_brands[..]);
    }
}
