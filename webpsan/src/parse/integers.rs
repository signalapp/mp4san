#![allow(missing_docs)]

use std::mem::size_of;
use std::num::NonZeroU32;

use bitflags::Flags;
use bytes::{Buf, BufMut};

use mediasan_common::error::WhileParsingType;
use mediasan_common::parse::FourCC;
use mediasan_common::{ensure_attach, report_attach, Result, ResultExt};

use super::ParseError;

pub trait WebmPrim: Sized {
    const ENCODED_LEN: u32;
    fn parse<B: Buf>(buf: B) -> Result<Self, ParseError>;
    fn put_buf<B: BufMut>(&self, buf: B);
}

pub trait WebmFlags: Flags {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct U24(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OneBasedU24(NonZeroU32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reserved<const LEN: u32>(());

//
// WebMPrim impls
//

macro_rules! webm_int {
    ($($ty:ty => ($get_fun:ident, $put_fun:ident)),+ $(,)?) => {
        $(impl WebmPrim for $ty {

            const ENCODED_LEN: u32 = size_of::<Self>() as u32;

            fn parse<B: Buf>(mut buf: B) -> Result<Self, ParseError> {
                ensure_attach!(
                    buf.remaining() >= Self::ENCODED_LEN as usize,
                    ParseError::TruncatedChunk,
                    WhileParsingType::new::<Self>(),
                );
                Ok(buf.$get_fun())
            }

            fn put_buf<B: BufMut>(&self, mut buf: B) {
                buf.$put_fun(*self)
            }
        })+
    };
}

webm_int! {
    u8 => (get_u8, put_u8),
    u16 => (get_u16, put_u16_le),
    u32 => (get_u32_le, put_u32_le),
    u64 => (get_u64_le, put_u64_le),
    i8 => (get_i8, put_i8),
    i16 => (get_i16_le, put_i16_le),
    i32 => (get_i32_le, put_i32_le),
    i64 => (get_i64_le, put_i64_le),
}

impl WebmPrim for FourCC {
    const ENCODED_LEN: u32 = 4;

    fn parse<B: Buf>(buf: B) -> Result<Self, ParseError> {
        ensure_attach!(
            buf.remaining() >= Self::ENCODED_LEN as usize,
            ParseError::TruncatedChunk,
            WhileParsingType::new::<Self>(),
        );
        Ok(FourCC::parse(buf))
    }

    fn put_buf<B: BufMut>(&self, buf: B) {
        FourCC::put_buf(self, buf)
    }
}

//
// WebmFlags impls
//

impl<T: WebmFlags> WebmPrim for T
where
    T::Bits: TryFrom<u64> + Into<u64>,
{
    const ENCODED_LEN: u32 = size_of::<<Self as Flags>::Bits>() as u32;

    fn parse<B: Buf>(mut buf: B) -> Result<Self, ParseError> {
        ensure_attach!(
            buf.remaining() >= Self::ENCODED_LEN as usize,
            ParseError::TruncatedChunk,
            WhileParsingType::new::<Self>(),
        );
        let value = buf.get_uint_le(Self::ENCODED_LEN as usize);
        let value = value.try_into().unwrap_or_else(|_| unreachable!());
        Self::from_bits(value)
            .ok_or_else(|| report_attach!(ParseError::InvalidInput, "non-zero reserved bits"))
            .while_parsing_type()
    }

    fn put_buf<B: BufMut>(&self, mut buf: B) {
        buf.put_uint(self.bits().into(), Self::ENCODED_LEN as usize);
    }
}

//
// U24 impls
//

impl U24 {
    pub fn get(&self) -> u32 {
        self.0
    }
}

impl WebmPrim for U24 {
    const ENCODED_LEN: u32 = 3;

    fn parse<B: Buf>(mut buf: B) -> Result<Self, ParseError> {
        ensure_attach!(
            buf.remaining() >= Self::ENCODED_LEN as usize,
            ParseError::TruncatedChunk,
            WhileParsingType::new::<Self>(),
        );
        Ok(Self(buf.get_uint_le(Self::ENCODED_LEN as usize) as u32))
    }

    fn put_buf<B: BufMut>(&self, mut buf: B) {
        buf.put_uint_le(self.0.into(), Self::ENCODED_LEN as usize)
    }
}

//
// OneBasedU24 impls
//

impl OneBasedU24 {
    pub fn get(&self) -> NonZeroU32 {
        self.0
    }
}

impl WebmPrim for OneBasedU24 {
    const ENCODED_LEN: u32 = 3;

    fn parse<B: Buf>(mut buf: B) -> Result<Self, ParseError> {
        ensure_attach!(
            buf.remaining() >= Self::ENCODED_LEN as usize,
            ParseError::TruncatedChunk,
            WhileParsingType::new::<Self>(),
        );
        let value = NonZeroU32::MIN.saturating_add(buf.get_uint_le(Self::ENCODED_LEN as usize) as u32);
        Ok(Self(value))
    }

    fn put_buf<B: BufMut>(&self, mut buf: B) {
        buf.put_uint_le(u64::from(self.0.get()) - 1, Self::ENCODED_LEN as usize);
    }
}

//
// Reserved impls
//

impl<const LEN: u32> WebmPrim for Reserved<LEN> {
    const ENCODED_LEN: u32 = LEN;

    fn parse<B: Buf>(mut buf: B) -> Result<Self, ParseError> {
        for _ in 0..LEN {
            ensure_attach!(
                buf.get_u8() == 0,
                ParseError::InvalidInput,
                "non-zero reserved bits",
                WhileParsingType::new::<Self>(),
            );
        }
        Ok(Self(()))
    }

    fn put_buf<B: BufMut>(&self, mut buf: B) {
        for _ in 0..LEN {
            buf.put_u8(0);
        }
    }
}
