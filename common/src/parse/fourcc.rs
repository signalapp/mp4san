use std::fmt;
use std::io;

use bytes::Buf;
use bytes::BufMut;
use futures_util::{pin_mut, AsyncRead, AsyncReadExt};

/// A four-byte character code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FourCC {
    /// The character code, as an array of four bytes.
    pub value: [u8; 4],
}

impl FourCC {
    /// The encoded length of a [`FourCC`], in bytes.
    pub const ENCODED_LEN: u32 = 4;

    /// Construct a [`FourCC`] from a string.
    pub const fn from_str(name: &str) -> Self {
        let name = name.as_bytes();
        let mut fourcc = [b' '; 4];
        let mut name_idx = 0;
        while name_idx < name.len() {
            fourcc[name_idx] = name[name_idx];
            name_idx += 1;
        }
        FourCC { value: fourcc }
    }

    /// Return the size of a [`FourCC`].
    pub const fn size() -> u64 {
        4
    }

    /// Read a [`FourCC`] from an [`AsyncRead`].
    pub async fn read<R: AsyncRead>(input: R) -> io::Result<Self> {
        let mut value = [0; 4];
        pin_mut!(input);
        input.read_exact(&mut value).await?;
        Ok(Self { value })
    }

    /// Parse a [`FourCC`] from a [`Buf`].
    ///
    /// The position of `input` is advanced by 4.
    ///
    /// # Panics
    ///
    /// This function panics if `input.remaining() < 4`.
    pub fn parse<B: Buf>(mut input: B) -> Self {
        let mut value = [0; 4];
        input.copy_to_slice(&mut value);
        Self { value }
    }

    /// Writes `self` to the [`BufMut`] `out`.
    pub fn put_buf<B: BufMut>(&self, mut out: B) {
        out.put(&self.value[..])
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
