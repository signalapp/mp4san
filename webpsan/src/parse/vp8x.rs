#![allow(missing_docs)]

use std::num::NonZeroU32;

use bytes::{BufMut, BytesMut};
use mediasan_common::parse::FourCC;
use mediasan_common::{ensure_attach, Result};

use super::chunk_type::VP8X;
use super::error::ParseResultExt;
use super::{OneBasedU24, ParseChunk, ParseError, ParsedChunk, Reserved, WebmFlags, WebmPrim};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vp8xChunk {
    pub flags: Vp8xFlags,
    reserved: Reserved<3>,
    canvas_width: OneBasedU24,
    canvas_height: OneBasedU24,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Vp8xFlags: u8 {
        const HAS_ICCP_CHUNK = 0b0010_0000;
        const HAS_ALPH_CHUNK = 0b0001_0000;
        const HAS_EXIF_CHUNK = 0b0000_1000;
        const HAS_XMP_CHUNK = 0b0000_0100;
        const IS_ANIMATED = 0b0000_0010;
    }
}

//
// Vp8xChunk impls
//

impl Vp8xChunk {
    pub fn canvas_width(&self) -> NonZeroU32 {
        self.canvas_width.get()
    }

    pub fn canvas_height(&self) -> NonZeroU32 {
        self.canvas_height.get()
    }
}

impl ParseChunk for Vp8xChunk {
    const NAME: FourCC = VP8X;

    const ENCODED_LEN: u32 =
        Vp8xFlags::ENCODED_LEN + Reserved::<3>::ENCODED_LEN + OneBasedU24::ENCODED_LEN + OneBasedU24::ENCODED_LEN;

    fn parse(mut buf: &mut BytesMut) -> Result<Self, ParseError> {
        let flags = Vp8xFlags::parse(&mut buf).while_parsing_field(Self::NAME, "flags")?;
        let reserved = Reserved::parse(&mut buf).while_parsing_field(Self::NAME, "reserved")?;
        let canvas_width = OneBasedU24::parse(&mut buf).while_parsing_field(Self::NAME, "canvas_width")?;
        let canvas_height = OneBasedU24::parse(&mut buf).while_parsing_field(Self::NAME, "canvas_height")?;
        ensure_attach!(
            canvas_height.get().checked_mul(canvas_width.get()).is_some(),
            ParseError::InvalidInput,
            "canvas pixel count overflow",
        );
        Ok(Self { flags, reserved, canvas_width, canvas_height })
    }
}

impl ParsedChunk for Vp8xChunk {
    fn put_buf(&self, mut buf: &mut dyn BufMut) {
        let Self { flags, reserved, canvas_width, canvas_height } = self;
        flags.put_buf(&mut buf);
        reserved.put_buf(&mut buf);
        canvas_width.put_buf(&mut buf);
        canvas_height.put_buf(&mut buf);
    }
}

//
// Vp8xFlags impls
//

impl WebmFlags for Vp8xFlags {}
