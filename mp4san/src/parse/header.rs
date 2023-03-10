use std::fmt;
use std::io;
use std::mem::size_of;

use bytes::{Buf, BufMut};
use derive_more::{Display, From};
use error_stack::Result;
use futures::{pin_mut, AsyncRead, AsyncReadExt, FutureExt};

use crate::sync::buf_async_reader;

use super::error::WhileParsingBox;
use super::{FourCC, Mpeg4Int, ParseError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxHeader {
    box_type: BoxType,
    box_size: BoxSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxSize {
    UntilEof,
    Size(u32),
    Ext(u64),
}

#[derive(Clone, Copy, Debug, Display, From, PartialEq, Eq)]
pub enum BoxType {
    FourCC(FourCC),
    Uuid(BoxUuid),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct BoxUuid(pub [u8; 16]);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FullBoxHeader {
    pub version: u8,
    pub flags: u32,
}

impl BoxHeader {
    pub const MAX_SIZE: u64 = 32;

    pub const fn with_u32_data_size(box_type: BoxType, data_size: u32) -> Self {
        let header_len = Self { box_type, box_size: BoxSize::Size(0) }.encoded_len() as u32;
        if let Some(box_size) = data_size.checked_add(header_len) {
            return Self { box_type, box_size: BoxSize::Size(box_size) };
        }

        let header_len = Self { box_type, box_size: BoxSize::Ext(0) }.encoded_len();
        Self { box_type, box_size: BoxSize::Ext(data_size as u64 + header_len) }
    }

    pub fn with_data_size(box_type: BoxType, data_size: u64) -> Result<Self, ParseError> {
        if data_size <= u32::MAX as u64 {
            return Ok(Self::with_u32_data_size(box_type, data_size as u32));
        }

        let header_len = Self { box_type, box_size: BoxSize::Ext(0) }.encoded_len();
        let Some(box_size) = data_size.checked_add(header_len) else {
            bail_attach!(ParseError::InvalidInput, "box size too large", WhileParsingBox(box_type));
        };
        Ok(Self { box_type, box_size: BoxSize::Ext(box_size) })
    }

    #[cfg(test)]
    pub const fn until_eof(box_type: BoxType) -> Self {
        Self { box_type, box_size: BoxSize::UntilEof }
    }

    pub fn parse<B: Buf + Unpin>(input: B) -> Result<Self, ParseError> {
        Self::read(buf_async_reader(input))
            .now_or_never()
            .unwrap()
            .map_err(|err| {
                assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
                report_attach!(ParseError::TruncatedBox, "while parsing box header")
            })
    }

    pub(crate) async fn read<R: AsyncRead>(input: R) -> io::Result<Self> {
        pin_mut!(input);

        let mut size = [0; 4];
        input.read_exact(&mut size).await?;

        let name = FourCC::read(&mut input).await?;

        let size = match u32::from_be_bytes(size) {
            0 => BoxSize::UntilEof,
            1 => {
                let mut size = [0; 8];
                input.read_exact(&mut size).await?;
                BoxSize::Ext(u64::from_be_bytes(size))
            }
            size => BoxSize::Size(size),
        };

        let name = match name {
            FourCC::UUID => {
                let mut uuid = [0; 16];
                input.read_exact(&mut uuid).await?;
                BoxType::Uuid(BoxUuid(uuid))
            }
            fourcc => fourcc.into(),
        };

        Ok(Self { box_type: name, box_size: size })
    }

    pub const fn encoded_len(&self) -> u64 {
        let mut size = FourCC::size() + size_of::<u32>() as u64;
        if let BoxSize::Ext(_) = self.box_size {
            size += size_of::<u64>() as u64;
        }
        if let BoxType::Uuid(_) = self.box_type {
            size += size_of::<BoxUuid>() as u64;
        }
        size
    }

    pub fn box_size(&self) -> Option<u64> {
        self.box_size.size()
    }

    pub fn box_data_size(&self) -> Result<Option<u64>, ParseError> {
        match self.box_size.size() {
            None => Ok(None),
            Some(size) => size
                .checked_sub(self.encoded_len())
                .ok_or_else(|| {
                    report_attach!(
                        ParseError::InvalidInput,
                        "box size too small",
                        WhileParsingBox(self.box_type)
                    )
                })
                .map(Some),
        }
    }

    pub const fn box_type(&self) -> BoxType {
        self.box_type
    }

    pub fn put_buf<B: BufMut>(&self, mut out: B) {
        match self.box_size {
            BoxSize::UntilEof => out.put_u32(0),
            BoxSize::Ext(_) => out.put_u32(1),
            BoxSize::Size(size) => out.put_u32(size),
        }

        match self.box_type {
            BoxType::FourCC(fourcc) => fourcc.put_buf(&mut out),
            BoxType::Uuid(_) => FourCC::UUID.put_buf(&mut out),
        }

        if let BoxSize::Ext(size) = self.box_size {
            out.put_u64(size);
        }

        if let BoxType::Uuid(uuid) = self.box_type {
            out.put(&uuid.0[..]);
        }
    }
}

impl BoxSize {
    pub const fn size(&self) -> Option<u64> {
        match *self {
            BoxSize::UntilEof => None,
            BoxSize::Size(size) => Some(size as u64),
            BoxSize::Ext(size) => Some(size),
        }
    }
}

macro_rules! box_type {
    ($($name:ident),+ $(,)?) => {
        impl FourCC {
            $(pub const $name: Self = box_name_to_fourcc(stringify!($name));)+
        }

        impl BoxType {
            $(pub const $name: Self = Self::FourCC(FourCC::$name);)+
        }

        pub mod box_type {
            use super::BoxType;
            $(pub const $name: BoxType = BoxType::$name;)+
        }
    };
}

box_type! {
    CO64,
    FREE,
    FTYP,
    MDAT,
    MDIA,
    MINF,
    MOOV,
    STBL,
    STCO,
    TRAK,
    UUID,
}

impl fmt::Display for BoxUuid {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self([a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p]) = *self;
        write!(
            fmt,
            "{a:02x}{b:02x}{c:02x}{d:02x}-{e:02x}{f:02x}-{g:02x}{h:02x}-{i:02x}{j:02x}-{k:02x}{l:02x}{m:02x}{n:02x}{o:02x}{p:02x}",
        )
    }
}

impl FullBoxHeader {
    pub const fn default() -> Self {
        Self { version: 0, flags: 0 }
    }

    pub fn parse<B: Buf>(mut buf: B) -> Result<Self, ParseError> {
        let version = u8::parse(&mut buf)?;
        let flags = <[u8; 3]>::parse(&mut buf)?;
        let flags = u32::from_be_bytes([0, flags[0], flags[1], flags[2]]);
        Ok(Self { version, flags })
    }

    pub const fn encoded_len(&self) -> u64 {
        4
    }

    pub fn ensure_eq(&self, other: &Self) -> Result<(), ParseError> {
        ensure_attach!(
            self.version == other.version,
            ParseError::InvalidInput,
            format!("box version {} does not match {}", self.version, other.version),
        );
        ensure_attach!(
            self.flags == other.flags,
            ParseError::InvalidInput,
            format!("box flags 0b{:024b} do not match 0b{:024b}", self.flags, other.flags),
        );
        Ok(())
    }

    pub fn put_buf<B: BufMut>(&self, mut out: B) {
        out.put_u8(self.version);
        out.put_uint(self.flags.into(), 3);
    }
}

const fn box_name_to_fourcc(name: &str) -> FourCC {
    let name = name.as_bytes();
    let mut fourcc = [b' '; 4];
    let mut name_idx = 0;
    while name_idx < name.len() {
        fourcc[name_idx] = name[name_idx].to_ascii_lowercase();
        name_idx += 1;
    }
    FourCC { value: fourcc }
}
