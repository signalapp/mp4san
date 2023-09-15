#![allow(missing_docs)]

use bytes::{BufMut, BytesMut};
use mediasan_common::parse::FourCC;
use mediasan_common::Result;

use super::chunk_type::ANIM;
use super::error::ParseResultExt;
use super::{ParseChunk, ParseError, ParsedChunk, WebmPrim};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimChunk {
    background_color: u32,
    loop_count: u16,
}

//
// AnimChunk impls
//

impl ParseChunk for AnimChunk {
    const NAME: FourCC = ANIM;

    const ENCODED_LEN: u32 = u32::ENCODED_LEN + u16::ENCODED_LEN;

    fn parse(mut buf: &mut BytesMut) -> Result<Self, ParseError> {
        let background_color = u32::parse(&mut buf).while_parsing_field(Self::NAME, "background_color")?;
        let loop_count = u16::parse(&mut buf).while_parsing_field(Self::NAME, "loop_count")?;
        Ok(Self { background_color, loop_count })
    }
}

impl ParsedChunk for AnimChunk {
    fn put_buf(&self, mut buf: &mut dyn BufMut) {
        let Self { background_color, loop_count } = self;
        background_color.put_buf(&mut buf);
        loop_count.put_buf(&mut buf);
    }
}
