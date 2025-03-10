#![allow(missing_docs)]

use std::fmt::Debug;
use std::io::{Cursor, Read};
use std::num::NonZeroU16;
use std::result::Result as StdResult;

use bitstream_io::{BitRead, BitReader, LE};
use bytes::{Buf, BytesMut};
use derive_more::Display;
use mediasan_common::ensure_attach;
use mediasan_common::parse::FourCC;
use mediasan_common::Result;

use crate::Error;

use super::bitstream::BitBufReader;
use super::chunk_type::VP8L;
use super::lossless::LosslessImage;
use super::{ParseChunk, ParseError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vp8lChunk {
    width: NonZeroU16,
    height: NonZeroU16,
    pub alpha_is_used: bool,
}

//
// private types
//

#[derive(Clone, Copy, Debug, Display)]
#[display("invalid VP8L signature `0x{_0:x}` != `0x{}`", Vp8lChunk::SIGNATURE)]
struct InvalidSignature(u8);

//
// Vp8lChunk impls
//

impl Vp8lChunk {
    const SIGNATURE: u8 = 0x2F;

    pub fn width(&self) -> NonZeroU16 {
        self.width
    }

    pub fn height(&self) -> NonZeroU16 {
        self.height
    }

    pub fn sanitize_image_data<R: Read>(&self, input: R) -> StdResult<(), Error> {
        let mut reader = BitBufReader::<_, LE>::with_capacity(input, 4096);
        let _image = LosslessImage::read(&mut reader, self.width.into(), self.height.into())?;
        Ok(())
    }
}

impl ParseChunk for Vp8lChunk {
    const NAME: FourCC = VP8L;

    const ENCODED_LEN: u32 = 5;

    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        let mut header = [0; Self::ENCODED_LEN as usize];
        buf.copy_to_slice(&mut header);
        let mut reader = BitReader::<_, LE>::new(Cursor::new(header));

        let signature = reader.read(8).unwrap();

        ensure_attach!(
            signature == Self::SIGNATURE,
            ParseError::InvalidInput,
            InvalidSignature(signature)
        );

        let width = NonZeroU16::MIN.saturating_add(reader.read(14).unwrap());
        let height = NonZeroU16::MIN.saturating_add(reader.read(14).unwrap());
        let alpha_is_used = reader.read_bit().unwrap();
        let version = reader.read(3).unwrap();

        ensure_attach!(version == 0, ParseError::UnsupportedVp8lVersion(version));

        Ok(Self { width, height, alpha_is_used })
    }
}
