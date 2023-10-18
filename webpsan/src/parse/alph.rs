#![allow(missing_docs)]

use std::io::Read;
use std::result::Result as StdResult;

use bitstream_io::LE;
use bytes::{BufMut, BytesMut};
use mediasan_common::parse::FourCC;
use mediasan_common::Result;

use crate::Error;

use super::bitstream::BitBufReader;
use super::chunk_type::ALPH;
use super::error::ParseResultExt;
use super::lossless::LosslessImage;
use super::{ParseChunk, ParseError, ParsedChunk, Vp8xChunk, WebmFlags, WebmPrim};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlphChunk {
    pub flags: AlphFlags,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct AlphFlags: u8 {
        const LEVEL_REDUCTION = 0b0001_0000;
        const FILTER_VERTICAL = 0b0000_1000;
        const FILTER_HORIZONTAL = 0b0000_0100;
        const COMPRESS_LOSSLESS = 0b0000_0001;
    }
}

//
// AlphChunk impls
//

impl AlphChunk {
    pub fn sanitize_image_data<R: Read>(&self, input: R, vp8x: &Vp8xChunk) -> StdResult<(), Error> {
        let (width, height) = (vp8x.canvas_width(), vp8x.canvas_height());
        if self.flags.contains(AlphFlags::COMPRESS_LOSSLESS) {
            let mut reader = BitBufReader::<_, LE>::with_capacity(input, 4096);
            let _image = LosslessImage::read(&mut reader, width, height)?;
        }
        Ok(())
    }
}

impl ParseChunk for AlphChunk {
    const NAME: FourCC = ALPH;

    const ENCODED_LEN: u32 = AlphFlags::ENCODED_LEN;

    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        let flags = AlphFlags::parse(buf).while_parsing_field(Self::NAME, "flags")?;
        Ok(Self { flags })
    }
}

impl ParsedChunk for AlphChunk {
    fn put_buf(&self, buf: &mut dyn BufMut) {
        let Self { flags } = self;
        flags.put_buf(buf);
    }
}

//
// AlphFlags impls
//

impl WebmFlags for AlphFlags {}
