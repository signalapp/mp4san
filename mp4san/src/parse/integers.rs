#![allow(missing_docs)]

use std::fmt;
use std::mem::size_of;

use bytes::{Buf, BufMut};
use mediasan_common::error::WhileParsingType;
use mediasan_common::ResultExt;

use crate::error::Result;

use super::error::WhereEq;
use super::{FourCC, Mp4ValueWriterExt, ParseError};

//
// types
//

pub trait Mp4Prim: Sized {
    fn parse<B: Buf + AsRef<[u8]>>(buf: B) -> Result<Self, ParseError>;
    fn encoded_len() -> u64;
    fn put_buf<B: BufMut>(&self, buf: B);
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConstU8<const N: u8 = 0>;

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConstU16<const N: u16 = 0>;

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConstU32<const N: u32 = 0>;

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConstU64<const N: u64 = 0>;

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConstI8<const N: i8 = 0>;

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConstI16<const N: i16 = 0>;

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConstI32<const N: i32 = 0>;

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConstI64<const N: i64 = 0>;

//
// Mp4Prim impls
//

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

impl<T: Mp4Prim, const N: usize> Mp4Prim for [T; N]
where
    [T; N]: Default,
{
    fn parse<B: Buf + AsRef<[u8]>>(mut buf: B) -> Result<Self, ParseError> {
        ensure_attach!(
            buf.remaining() >= Self::encoded_len() as usize,
            ParseError::TruncatedBox,
            WhileParsingType::new::<Self>(),
        );
        let mut parsed: [T; N] = Default::default();
        for value in &mut parsed {
            *value = T::parse(&mut buf)?;
        }
        Ok(parsed)
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

//
// Const* impls
//

macro_rules! mp4_const_int {
    ($($ident:ident => $ty:ty),+ $(,)?) => {
        $(
            impl<const N: $ty> Mp4Prim for $ident<N> {
                fn parse<B: Buf + AsRef<[u8]>>(mut buf: B) -> Result<Self, ParseError> {
                    ensure_attach!(
                        <$ty>::parse(buf.as_ref())? == N,
                        ParseError::InvalidInput,
                        WhereEq(stringify!(N), N)
                    );
                    buf.advance(Self::encoded_len() as usize);
                    Ok(Self)
                }

                fn encoded_len() -> u64 {
                    <$ty>::encoded_len()
                }

                fn put_buf<B: BufMut>(&self, buf: B) {
                    N.put_buf(buf)
                }
            }

            impl<const N: $ty> fmt::Debug for $ident<N> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.debug_struct(&format!(concat!(stringify!($ident), "<{}>"), N))
                     .finish()
                }
            }
        )+
    };
}

mp4_const_int! {
    ConstU8 => u8,
    ConstU16 => u16,
    ConstU32 => u32,
    ConstU64 => u64,
    ConstI8 => i8,
    ConstI16 => i16,
    ConstI32 => i32,
    ConstI64 => i64,
}
