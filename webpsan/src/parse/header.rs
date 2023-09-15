use std::io;

use assert_matches::assert_matches;
use bytes::{Buf, BufMut, BytesMut};
use futures_util::{AsyncRead, AsyncReadExt};
use mediasan_common::error::WhileParsingType;
use mediasan_common::{ensure_attach, Result};

use super::{FourCC, ParseChunk, ParseError, ParsedChunk, WebmPrim};

#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkHeader {
    pub name: FourCC,
    pub len: u32,
}

#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WebpChunk;

macro_rules! chunk_type {
    ($($code:ident),+ $(,)?) => {
        #[allow(missing_docs)]
        pub mod chunk_type {
            use super::*;
            $(
                #[doc = concat!("The `", stringify!($code), "` chunk type.")]
                pub const $code: FourCC = FourCC::from_str(stringify!($code));
            )+
        }
    };
}

chunk_type!(ALPH, ANIM, ANMF, EXIF, ICCP, RIFF, VP8, VP8L, VP8X, XMP);

//
// ChunkHeader impls
//

#[allow(missing_docs)]
impl ChunkHeader {
    pub fn padded(&self) -> bool {
        // RIFF chunks are padded to an even length
        self.len % 2 == 1
    }

    pub(crate) async fn read<R: AsyncRead + Unpin>(mut input: R) -> io::Result<Self> {
        let mut buf = [0; Self::ENCODED_LEN as usize];
        input.read_exact(&mut buf).await?;
        let value = Self::parse(&buf[..]).map_err(|err| {
            assert_matches!(err.into_inner(), ParseError::TruncatedChunk);
            io::ErrorKind::UnexpectedEof
        })?;
        Ok(value)
    }
}

impl WebmPrim for ChunkHeader {
    const ENCODED_LEN: u32 = 8;

    fn parse<B: Buf>(mut buf: B) -> Result<Self, ParseError> {
        ensure_attach!(
            buf.remaining() >= Self::ENCODED_LEN as usize,
            ParseError::TruncatedChunk,
            WhileParsingType::new::<Self>()
        );

        let name = FourCC::parse(&mut buf);
        let len = buf.get_u32_le();
        Ok(Self { name, len })
    }

    fn put_buf<B: BufMut>(&self, mut buf: B) {
        self.name.put_buf(&mut buf);
        buf.put_u32_le(self.len);
    }
}

//
// WebpChunk impls
//

#[allow(missing_docs)]
impl WebpChunk {
    pub const WEBP: FourCC = FourCC::from_str("WEBP");
}

impl ParseChunk for WebpChunk {
    const ENCODED_LEN: u32 = FourCC::ENCODED_LEN;
    const NAME: FourCC = chunk_type::RIFF;

    fn parse(input: &mut BytesMut) -> Result<Self, ParseError> {
        let name = FourCC::parse(input);

        ensure_attach!(
            name == Self::WEBP,
            ParseError::InvalidInput,
            "not a WebP file",
            WhileParsingType::new::<Self>(),
        );

        Ok(Self)
    }
}

impl ParsedChunk for WebpChunk {
    fn put_buf(&self, out: &mut dyn BufMut) {
        Self::WEBP.put_buf(out);
    }
}
