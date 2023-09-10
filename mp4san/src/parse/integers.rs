#![allow(missing_docs)]

use std::mem::size_of;

use bytes::Buf;
use bytes::BufMut;
use mediasan_common::error::WhileParsingType;
use mediasan_common::ResultExt;

use crate::error::Result;

use super::{FourCC, Mp4ValueWriterExt, ParseError};

pub trait Mp4Prim: Sized {
    fn parse<B: Buf + AsRef<[u8]>>(buf: B) -> Result<Self, ParseError>;
    fn encoded_len() -> u64;
    fn put_buf<B: BufMut>(&self, buf: B);
}

macro_rules! mp4_int {
    ($($ty:ty => ($get_fun:ident, $put_fun:ident)),+ $(,)?) => {
        $(impl Mp4Prim for $ty {
            fn parse<B: Buf>(mut buf: B) -> Result<Self, ParseError> {
                if buf.remaining() < Self::encoded_len() as usize {
                    bail_attach!(ParseError::TruncatedBox, WhileParsingType::new::<$ty>());
                }
                Ok(buf.$get_fun())
            }

            fn encoded_len() -> u64 {
                size_of::<Self>() as u64
            }

            fn put_buf<B: BufMut>(&self, mut buf: B) {
                buf.$put_fun(*self)
            }
        })+
    };
}

mp4_int! {
    u8 => (get_u8, put_u8),
    u16 => (get_u16, put_u16),
    u32 => (get_u32, put_u32),
    u64 => (get_u64, put_u64),
    i8 => (get_i8, put_i8),
    i16 => (get_i16, put_i16),
    i32 => (get_i32, put_i32),
    i64 => (get_i64, put_i64),
}

impl<T: Mp4Prim, const N: usize> Mp4Prim for [T; N] {
    fn parse<B: Buf + AsRef<[u8]>>(mut buf: B) -> Result<Self, ParseError> {
        ensure_attach!(
            buf.remaining() >= Self::encoded_len() as usize,
            ParseError::TruncatedBox,
            WhileParsingType::new::<Self>(),
        );
        Ok([(); N].map(|()| T::parse(&mut buf).unwrap_or_else(|_| unreachable!())))
    }

    fn encoded_len() -> u64 {
        size_of::<Self>() as u64
    }

    fn put_buf<B: BufMut>(&self, mut buf: B) {
        for value in self {
            buf.put_mp4_value(value);
        }
    }
}

impl Mp4Prim for FourCC {
    fn parse<B: Buf + AsRef<[u8]>>(buf: B) -> Result<Self, ParseError> {
        Mp4Prim::parse(buf).map(|value| Self { value }).while_parsing_type()
    }

    fn encoded_len() -> u64 {
        Self::size()
    }

    fn put_buf<B: BufMut>(&self, mut buf: B) {
        buf.put_mp4_value(&self.value);
    }
}
