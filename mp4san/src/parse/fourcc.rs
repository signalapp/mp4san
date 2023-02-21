use std::fmt;
use std::io;
use std::io::Read;

use bytes::{Buf, BufMut};

use super::{Mpeg4Int, ParseError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FourCC {
    pub value: [u8; 4],
}

impl FourCC {
    pub const fn size() -> u64 {
        4
    }

    pub fn read<R: Read>(mut input: R) -> Result<Self, io::Error> {
        let mut value = [0; 4];
        input.read_exact(&mut value)?;
        Ok(Self { value })
    }

    pub fn put_buf<B: BufMut>(&self, mut out: B) {
        out.put(&self.value[..])
    }
}

impl Mpeg4Int for FourCC {
    fn parse<B: Buf>(buf: B) -> Result<Self, ParseError> {
        Ok(FourCC { value: Mpeg4Int::parse(buf)? })
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
