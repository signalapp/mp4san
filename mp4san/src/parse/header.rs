use std::fmt;
use std::io;
use std::mem::size_of;

use bytes::{Buf, BufMut};
use derive_more::{Display, From};
use futures_util::{pin_mut, AsyncRead, AsyncReadExt, FutureExt};

use crate::error::Result;
use crate::sync::buf_async_reader;

use super::error::WhileParsingBox;
use super::{FourCC, Mp4Prim, ParseError};

#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxHeader {
    box_type: BoxType,
    box_size: BoxSize,
}

#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxSize {
    UntilEof,
    Size(u32),
    Ext(u64),
}

/// An MP4 box type.
#[derive(Clone, Copy, Debug, Display, From, PartialEq, Eq)]
pub enum BoxType {
    /// A box type in four-byte character code form.
    FourCC(FourCC),

    /// A box type in UUID form.
    Uuid(BoxUuid),
}

/// An MP4 box type as a UUID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct BoxUuid {
    /// The UUID, as an array of 16 bytes.
    pub value: [u8; 16],
}

#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FullBoxHeader {
    pub version: u8,
    pub flags: u32,
}

#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ConstFullBoxHeader<const VERSION: u8 = 0, const FLAGS: u32 = 0>;

#[allow(missing_docs)]
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
            bail_attach!(
                ParseError::InvalidInput,
                "box size too large",
                WhileParsingBox(box_type)
            );
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
            fourcc::UUID => {
                let mut uuid = [0; 16];
                input.read_exact(&mut uuid).await?;
                BoxType::Uuid(BoxUuid { value: uuid })
            }
            fourcc => fourcc.into(),
        };

        Ok(Self { box_type: name, box_size: size })
    }

    pub fn overwrite_size(&mut self, actual_box_size: u32) {
        assert_eq!(self.box_size, BoxSize::UntilEof);
        self.box_size = BoxSize::Size(actual_box_size);
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
            BoxType::Uuid(_) => fourcc::UUID.put_buf(&mut out),
        }

        if let BoxSize::Ext(size) = self.box_size {
            out.put_u64(size);
        }

        if let BoxType::Uuid(uuid) = self.box_type {
            out.put(&uuid.value[..]);
        }
    }
}

#[allow(missing_docs)]
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
        paste::paste! {
            #[allow(missing_docs)]
            pub mod fourcc {
                use super::*;
                $(
                    #[doc = concat!("The `", stringify!([<$name:lower>]), "` box type.")]
                    pub const $name: FourCC = FourCC::from_str(stringify!([<$name:lower>]));
                )+
            }

            impl BoxType {
                $(
                    #[doc = concat!("The `", stringify!([<$name:lower>]), "` box type.")]
                    pub const $name: Self = Self::FourCC(fourcc::$name);
                )+
            }

            /// [`BoxType`] constants.
            pub mod box_type {
                use super::BoxType;
                $(
                    #[doc = concat!("The `", stringify!([<$name:lower>]), "` box type.")]
                    pub const $name: BoxType = BoxType::$name;
                )+
            }
        }
    };
}

box_type! {
    CO64,
    DINF,
    DREF,
    FREE,
    FTYP,
    HDLR,
    MDAT,
    MDHD,
    MDIA,
    MECO,
    META,
    METT,
    MINF,
    MOOV,
    MVHD,
    SKIP,
    STBL,
    STCO,
    STSC,
    STSD,
    STSZ,
    STTS,
    TKHD,
    TRAK,
    URL,
    UUID,
}

impl fmt::Display for BoxUuid {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { value: [a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p] } = *self;
        write!(
            fmt,
            "{a:02x}{b:02x}{c:02x}{d:02x}-{e:02x}{f:02x}-{g:02x}{h:02x}-{i:02x}{j:02x}-{k:02x}{l:02x}{m:02x}{n:02x}{o:02x}{p:02x}",
        )
    }
}

#[allow(missing_docs)]
impl FullBoxHeader {
    pub const fn default() -> Self {
        Self { version: 0, flags: 0 }
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
}

impl Mp4Prim for FullBoxHeader {
    fn parse<B: Buf>(mut buf: B) -> Result<Self, ParseError> {
        let version = u8::parse(&mut buf)?;
        let flags = <[u8; 3]>::parse(buf)?;
        let flags = u32::from_be_bytes([0, flags[0], flags[1], flags[2]]);
        Ok(Self { version, flags })
    }

    fn encoded_len() -> u64 {
        4
    }

    fn put_buf<B: BufMut>(&self, mut out: B) {
        out.put_u8(self.version);
        out.put_uint(self.flags.into(), 3);
    }
}

impl<const VERSION: u8, const FLAGS: u32> From<ConstFullBoxHeader<VERSION, FLAGS>> for FullBoxHeader {
    fn from(_: ConstFullBoxHeader<VERSION, FLAGS>) -> Self {
        Self { version: VERSION, flags: FLAGS }
    }
}

impl<const VERSION: u8, const FLAGS: u32> Mp4Prim for ConstFullBoxHeader<VERSION, FLAGS> {
    fn parse<B: Buf>(buf: B) -> Result<Self, ParseError> {
        FullBoxHeader::parse(buf)?.ensure_eq(&Self.into())?;
        Ok(Self)
    }

    fn encoded_len() -> u64 {
        FullBoxHeader::encoded_len()
    }

    fn put_buf<B: BufMut>(&self, mut out: B) {
        out.put_u8(VERSION);
        out.put_uint(FLAGS.into(), 3);
    }
}
