#![allow(missing_docs)]

use std::fmt;
use std::mem::size_of;
use std::num::{NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU8};

use bytes::{Buf, BufMut};
use fixed::traits::Fixed;
use fixed::types::{I16F16, I2F30};
use fixed::{FixedI16, FixedI32, FixedI64, FixedI8, FixedU16, FixedU32, FixedU64, FixedU8};
use mediasan_common::error::WhileParsingType;
use mediasan_common::ResultExt;
use nalgebra::{Matrix3, Matrix3x2, Vector3};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mp4Transform {
    pub transform: Matrix3x2<I16F16>,
    pub normalizer: Vector3<I2F30>,
}

trait Mp4Fixed {}

//
// Mp4Prim impls
//

macro_rules! mp4_int {
    ($($ty:ty, $nonzero_ty:ty => ($get_fun:ident, $put_fun:ident)),+ $(,)?) => {
        $(impl Mp4Prim for $ty {
            fn parse<B: Buf>(mut buf: B) -> Result<Self, ParseError> {
                ensure_attach!(
                    buf.remaining() >= Self::encoded_len() as usize,
                    ParseError::TruncatedBox,
                    WhileParsingType::new::<$ty>(),
                );
                Ok(buf.$get_fun())
            }

            fn encoded_len() -> u64 {
                size_of::<Self>() as u64
            }

            fn put_buf<B: BufMut>(&self, mut buf: B) {
                buf.$put_fun(*self)
            }
        })+

        $(impl Mp4Prim for $nonzero_ty {
            fn parse<B: Buf>(mut buf: B) -> Result<Self, ParseError> {
                ensure_attach!(
                    buf.remaining() >= Self::encoded_len() as usize,
                    ParseError::TruncatedBox,
                    WhileParsingType::new::<$ty>(),
                );
                let Some(value) = <$nonzero_ty>::new(buf.$get_fun()) else {
                    bail_attach!(
                        ParseError::InvalidInput,
                        WhileParsingType::new::<$nonzero_ty>(),
                    );
                };
                Ok(value)
            }

            fn encoded_len() -> u64 {
                size_of::<Self>() as u64
            }

            fn put_buf<B: BufMut>(&self, mut buf: B) {
                buf.$put_fun(self.get())
            }
        })+
    };
}

mp4_int! {
    u8, NonZeroU8 => (get_u8, put_u8),
    u16, NonZeroU16 => (get_u16, put_u16),
    u32, NonZeroU32 => (get_u32, put_u32),
    u64, NonZeroU64 => (get_u64, put_u64),
    i8, NonZeroI8 => (get_i8, put_i8),
    i16, NonZeroI16 => (get_i16, put_i16),
    i32, NonZeroI32 => (get_i32, put_i32),
    i64, NonZeroI64 => (get_i64, put_i64),
}

impl<T: Fixed + Mp4Fixed> Mp4Prim for T
where
    T::Bits: Mp4Prim,
{
    fn parse<B: Buf + AsRef<[u8]>>(buf: B) -> Result<Self, ParseError> {
        let bits = <T::Bits>::parse(buf)?;
        Ok(T::from_bits(bits))
    }

    fn encoded_len() -> u64 {
        <T::Bits>::encoded_len()
    }

    fn put_buf<B: BufMut>(&self, buf: B) {
        self.to_bits().put_buf(buf)
    }
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

//
// Mp4Transform impls
//

impl Mp4Transform {
    pub const UNITY: Self = Self {
        transform: Matrix3x2::new(
            I16F16::ONE,
            I16F16::ZERO,
            I16F16::ZERO,
            I16F16::ZERO,
            I16F16::ONE,
            I16F16::ZERO,
        ),
        normalizer: Vector3::new(I2F30::ZERO, I2F30::ZERO, I2F30::ONE),
    };
}

impl Default for Mp4Transform {
    fn default() -> Self {
        Self::UNITY
    }
}

impl Mp4Prim for Mp4Transform {
    fn parse<B: Buf + AsRef<[u8]>>(mut buf: B) -> Result<Self, ParseError> {
        let mut raw = Matrix3::default();
        for value in &mut raw {
            *value = Mp4Prim::parse(&mut buf)?;
        }
        Ok(Self {
            transform: raw.fixed_columns::<2>(0).map(I16F16::from_bits),
            normalizer: raw.column(2).map(I2F30::from_bits),
        })
    }

    fn encoded_len() -> u64 {
        <[i32; 9]>::encoded_len()
    }

    fn put_buf<B: BufMut>(&self, mut buf: B) {
        let raw = Matrix3::from_columns(&[
            self.transform.column(0).map(I16F16::to_bits),
            self.transform.column(1).map(I16F16::to_bits),
            self.normalizer.map(I2F30::to_bits),
        ]);
        for row in raw.row_iter() {
            for value in &row {
                buf.put_mp4_value(value);
            }
        }
    }
}

//
// Mp4Fixed impls
//

impl<Frac> Mp4Fixed for FixedI8<Frac> {}
impl<Frac> Mp4Fixed for FixedI16<Frac> {}
impl<Frac> Mp4Fixed for FixedI32<Frac> {}
impl<Frac> Mp4Fixed for FixedI64<Frac> {}
impl<Frac> Mp4Fixed for FixedU8<Frac> {}
impl<Frac> Mp4Fixed for FixedU16<Frac> {}
impl<Frac> Mp4Fixed for FixedU32<Frac> {}
impl<Frac> Mp4Fixed for FixedU64<Frac> {}
