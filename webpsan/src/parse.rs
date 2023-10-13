//! Unstable API for parsing WebP files.

mod alph;
mod anim;
mod anmf;
mod bitstream;
pub mod error;
mod header;
mod integers;
mod lossless;
mod vp8l;
mod vp8x;

use bytes::{BufMut, BytesMut};
use mediasan_common::Result;

#[allow(missing_docs)]
pub trait ParseChunk: Sized {
    const NAME: FourCC;

    const ENCODED_LEN: u32;

    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError>;
}

#[allow(missing_docs)]
pub trait ParsedChunk {
    fn put_buf(&self, buf: &mut dyn BufMut);
}

pub use alph::{AlphChunk, AlphFlags};
pub use anim::AnimChunk;
pub use anmf::{AnmfChunk, AnmfFlags};
pub use bitstream::{BitBufReader, CanonicalHuffmanTree};
pub use error::ParseError;
pub use header::{chunk_type, ChunkHeader, WebpChunk};
pub use integers::{OneBasedU24, Reserved, WebmFlags, WebmPrim, U24};
pub use lossless::LosslessImage;
pub use vp8l::Vp8lChunk;
pub use vp8x::{Vp8xChunk, Vp8xFlags};

pub use mediasan_common::parse::FourCC;
