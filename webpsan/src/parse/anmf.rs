#![allow(missing_docs)]

use std::num::NonZeroU32;

use bytes::{BufMut, BytesMut};
use mediasan_common::parse::FourCC;
use mediasan_common::Result;

use super::chunk_type::ANMF;
use super::error::ParseResultExt;
use super::{OneBasedU24, ParseChunk, ParseError, ParsedChunk, WebmFlags, WebmPrim, U24};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AnmfChunk {
    x: U24,
    y: U24,
    width: OneBasedU24,
    height: OneBasedU24,
    duration: U24,
    pub flags: AnmfFlags,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct AnmfFlags: u8 {
        const ALPHA_BLENDING = 0b0000_0010;
        const DISPOSE_BACKGROUND = 0b0000_0001;
    }
}

//
// AnmfChunk impls
//

impl AnmfChunk {
    pub fn x(&self) -> u32 {
        self.x.get()
    }

    pub fn y(&self) -> u32 {
        self.y.get()
    }

    pub fn width(&self) -> NonZeroU32 {
        self.width.get()
    }

    pub fn height(&self) -> NonZeroU32 {
        self.height.get()
    }

    pub fn duration(&self) -> u32 {
        self.duration.get()
    }
}

impl ParseChunk for AnmfChunk {
    const NAME: FourCC = ANMF;

    const ENCODED_LEN: u32 = U24::ENCODED_LEN
        + U24::ENCODED_LEN
        + OneBasedU24::ENCODED_LEN
        + OneBasedU24::ENCODED_LEN
        + U24::ENCODED_LEN
        + AnmfFlags::ENCODED_LEN;

    fn parse(mut buf: &mut BytesMut) -> Result<Self, ParseError> {
        let x = U24::parse(&mut buf).while_parsing_field(Self::NAME, "x")?;
        let y = U24::parse(&mut buf).while_parsing_field(Self::NAME, "y")?;
        let width = OneBasedU24::parse(&mut buf).while_parsing_field(Self::NAME, "width")?;
        let height = OneBasedU24::parse(&mut buf).while_parsing_field(Self::NAME, "height")?;
        let duration = U24::parse(&mut buf).while_parsing_field(Self::NAME, "duration")?;
        let flags = AnmfFlags::parse(&mut buf).while_parsing_field(Self::NAME, "flags")?;
        Ok(Self { x, y, width, height, duration, flags })
    }
}

impl ParsedChunk for AnmfChunk {
    fn put_buf(&self, mut buf: &mut dyn BufMut) {
        let Self { x, y, width, height, duration, flags } = self;
        x.put_buf(&mut buf);
        y.put_buf(&mut buf);
        width.put_buf(&mut buf);
        height.put_buf(&mut buf);
        duration.put_buf(&mut buf);
        flags.put_buf(&mut buf);
    }
}

//
// AnmfFlags impls
//

impl WebmFlags for AnmfFlags {}
