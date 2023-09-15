use std::io::Cursor;
use std::mem::replace;
use std::num::NonZeroU32;

use bitstream_io::huffman::ReadHuffmanTree;
use bitstream_io::{BitRead, BitReader, Endianness, HuffmanRead, Numeric};
use derive_more::Display;
use futures_util::{AsyncRead, AsyncReadExt};
use mediasan_common::util::IoResultExt;
use mediasan_common::{bail_attach, report_attach};

use crate::parse::ParseError;
use crate::Error;

pub struct BitBufReader<R, E: Endianness> {
    input: R,
    reader: BitReader<Cursor<Vec<u8>>, E>,
    buf_len: usize,
}

#[derive(Display)]
#[display(fmt = "invalid lz77 prefix code `{_0}`")]
struct InvalidLz77PrefixCode(u16);

impl<R: AsyncRead + Unpin, E: Endianness> BitBufReader<R, E> {
    pub fn with_capacity(input: R, capacity: usize) -> Self {
        Self { input, reader: BitReader::new(Cursor::new(Vec::with_capacity(capacity))), buf_len: 0 }
    }

    pub async fn ensure_bits(&mut self, bits: u32) -> Result<(), Error> {
        let bit_pos = self.reader.position_in_bits()?;
        let byte_pos = (bit_pos / 8) as usize;
        if self.buf_len as u64 * 8 - bit_pos < bits.into() {
            let reader = replace(&mut self.reader, BitReader::new(Cursor::new(Vec::new())));
            let mut buf = reader.into_reader().into_inner();

            buf.drain(..byte_pos);
            (&mut self.input)
                .take((buf.capacity() - buf.len()) as u64)
                .read_to_end(&mut buf)
                .await?;
            self.buf_len = buf.len();

            self.reader = BitReader::new(Cursor::new(buf));
            self.reader.skip((bit_pos % 8) as u32)?;
        }
        Ok(())
    }

    pub async fn read_bit(&mut self) -> Result<bool, Error> {
        self.ensure_bits(1).await?;
        self.reader
            .read_bit()
            .map_eof(|_| Error::Parse(report_attach!(ParseError::TruncatedChunk)))
    }

    pub async fn read<T: Numeric>(&mut self, bits: u32) -> Result<T, Error> {
        self.ensure_bits(bits).await?;
        self.reader
            .read(bits)
            .map_eof(|_| Error::Parse(report_attach!(ParseError::TruncatedChunk)))
    }

    pub async fn read_huffman<T: Clone>(&mut self, tree: &[ReadHuffmanTree<E, T>]) -> Result<T, Error> {
        // XXX calculate longest huffman code
        self.ensure_bits(128).await?;
        self.reader
            .read_huffman(tree)
            .map_eof(|_| Error::Parse(report_attach!(ParseError::TruncatedChunk)))
    }

    pub async fn read_lz77(&mut self, prefix_code: u16) -> Result<NonZeroU32, Error> {
        match prefix_code {
            0..=3 => Ok(NonZeroU32::MIN.saturating_add(prefix_code.into())),
            4..=39 => {
                let extra_bits = (u32::from(prefix_code) - 2) >> 1;
                let offset = (2 + (u32::from(prefix_code) & 1)) << extra_bits;
                Ok(NonZeroU32::MIN.saturating_add(offset + self.read::<u32>(extra_bits).await?))
            }
            _ => bail_attach!(ParseError::InvalidInput, InvalidLz77PrefixCode(prefix_code)),
        }
    }
}
