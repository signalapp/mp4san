use std::fmt;
use std::io;

use bytes::{Buf, BufMut};
use error_stack::Result;
use futures::{pin_mut, AsyncRead, AsyncReadExt};

use super::error::ParseResultExt;
use super::integers::Mpeg4IntWriterExt;
use super::{Mpeg4Int, ParseError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FourCC {
    pub value: [u8; 4],
}

impl FourCC {
    pub const fn size() -> u64 {
        4
    }

    pub(crate) async fn read<R: AsyncRead>(input: R) -> io::Result<Self> {
        let mut value = [0; 4];
        pin_mut!(input);
        input.read_exact(&mut value).await?;
        Ok(Self { value })
    }

    pub fn put_buf<B: BufMut>(&self, mut out: B) {
        out.put(&self.value[..])
    }
}

impl Mpeg4Int for FourCC {
    fn parse<B: Buf>(buf: B) -> Result<Self, ParseError> {
        Ok(FourCC { value: Mpeg4Int::parse(buf).while_parsing_type::<Self>()? })
    }

    fn put_buf<B: BufMut>(&self, mut buf: B) {
        buf.put_mp4int(self.value);
    }
}

impl fmt::Display for FourCC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Ok(string) = std::str::from_utf8(&self.value) {
            let string = string.trim();
            write!(f, "{string}")
        } else {
            write!(f, "0x{:08x}", u32::from_be_bytes(self.value))
        }
    }
}
