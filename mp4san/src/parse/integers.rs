#![allow(missing_docs)]

use std::mem::size_of;

use bytes::Buf;
use bytes::BufMut;
use error_stack::Result;

use super::error::WhileParsingType;
use super::ParseError;

pub trait Mpeg4Int: Clone + Copy + Sized {
    fn parse<B: Buf>(buf: B) -> Result<Self, ParseError>;
    fn put_buf<B: BufMut>(&self, buf: B);
}

pub trait Mpeg4IntReaderExt: Buf {
    fn get<T: Mpeg4Int>(&mut self) -> Result<T, ParseError> {
        T::parse(self)
    }
}

pub trait Mpeg4IntWriterExt: BufMut {
    fn put_mp4int<T: Mpeg4Int>(&mut self, value: T) {
        value.put_buf(self)
    }
}

macro_rules! mpeg4_int {
    ($($ty:ty => ($get_fun:ident, $put_fun:ident)),+ $(,)?) => {
        $(impl Mpeg4Int for $ty {
            fn parse<B: Buf>(mut buf: B) -> Result<Self, ParseError> {
                if buf.remaining() < size_of::<Self>() {
                    use crate::parse::error::WhileParsingType;
                    bail_attach!(ParseError::TruncatedBox, WhileParsingType(stringify!($ty)));
                }
                Ok(buf.$get_fun())
            }

            fn put_buf<B: BufMut>(&self, mut buf: B) {
                buf.$put_fun(*self)
            }
        })+
    };
}

mpeg4_int! {
    u8 => (get_u8, put_u8),
    u16 => (get_u16, put_u16),
    u32 => (get_u32, put_u32),
    u64 => (get_u64, put_u64),
    i8 => (get_i8, put_i8),
    i16 => (get_i16, put_i16),
    i32 => (get_i32, put_i32),
    i64 => (get_i64, put_i64),
}

impl<T: Mpeg4Int, const N: usize> Mpeg4Int for [T; N] {
    fn parse<B: Buf>(mut buf: B) -> Result<Self, ParseError> {
        ensure_attach!(
            buf.remaining() >= size_of::<Self>(),
            ParseError::TruncatedBox,
            WhileParsingType::new::<Self>(),
        );
        Ok([(); N].map(|()| T::parse(&mut buf).unwrap_or_else(|_| unreachable!())))
    }

    fn put_buf<B: BufMut>(&self, mut buf: B) {
        for value in self {
            buf.put_mp4int(*value);
        }
    }
}

impl<T: Buf> Mpeg4IntReaderExt for T {}

impl<T: BufMut> Mpeg4IntWriterExt for T {}
