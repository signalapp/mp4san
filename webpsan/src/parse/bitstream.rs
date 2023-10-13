#![allow(missing_docs)]

use std::fmt::Debug;
use std::io::Cursor;
use std::mem::replace;
use std::num::NonZeroU32;

use bitstream_io::huffman::{compile_read_tree, ReadHuffmanTree};
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

pub struct CanonicalHuffmanTree<E: Endianness, S: Clone> {
    read_tree: Box<[ReadHuffmanTree<E, S>]>,
    longest_code_len: u32,
}

#[derive(Display)]
#[display(fmt = "invalid lz77 prefix code `{_0}`")]
struct InvalidLz77PrefixCode(u16);

//
// BitBufReader impls
//

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

    pub async fn read_huffman<T: Clone>(&mut self, tree: &CanonicalHuffmanTree<E, T>) -> Result<T, Error> {
        self.ensure_bits(tree.longest_code_len).await?;
        self.reader
            .read_huffman(&tree.read_tree)
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

    pub fn reader(&mut self) -> &mut BitReader<Cursor<Vec<u8>>, E> {
        &mut self.reader
    }
}

//
// CanonicalHuffmanTree impls
//

impl<E: Endianness, S: Clone> CanonicalHuffmanTree<E, S> {
    pub fn new(code_lengths: &mut [(S, u8)]) -> Result<Self, Error>
    where
        S: Copy + Debug + Ord + 'static,
    {
        let longest_code_len = u32::from(code_lengths.iter().map(|&(_, len)| len).max().unwrap_or_default());
        let symbols = Self::symbols(code_lengths);
        log::debug!("symbols: {symbols:?}");
        let read_tree =
            compile_read_tree(symbols).map_err(|err| report_attach!(ParseError::InvalidVp8lPrefixCode, err))?;
        Ok(Self { read_tree, longest_code_len })
    }

    pub fn from_symbols(symbols: Vec<(S, Vec<u8>)>) -> Result<Self, Error> {
        let longest_code_len = symbols.iter().map(|(_, code)| code.len()).max().unwrap_or_default() as u32;
        let read_tree =
            compile_read_tree(symbols).map_err(|err| report_attach!(ParseError::InvalidVp8lPrefixCode, err))?;
        Ok(Self { read_tree, longest_code_len })
    }

    pub fn read_tree(&self) -> &[ReadHuffmanTree<E, S>] {
        &self.read_tree
    }

    pub fn longest_code_len(&self) -> u32 {
        self.longest_code_len
    }

    fn symbols(code_lengths: &mut [(S, u8)]) -> Vec<(S, Vec<u8>)>
    where
        S: Copy + Ord + 'static,
    {
        code_lengths.sort_unstable_by_key(|&(symbol, code_length)| (code_length, symbol));
        let zero_code_length_count = code_lengths.partition_point(|&(_, code_length)| code_length == 0);

        match (&code_lengths[zero_code_length_count..], &*code_lengths) {
            (&[(first_symbol, 1)], _) => vec![(first_symbol, vec![])],

            (&[(first_symbol, first_code_length), ref rest_code_lengths @ ..], &[.., (_, last_code_length)]) => {
                let mut code = Vec::with_capacity(last_code_length.into());
                code.resize(first_code_length.into(), 0);

                let mut symbols = Vec::with_capacity(code_lengths.len());
                symbols.push((first_symbol, code.clone()));
                for &(symbol, code_length) in rest_code_lengths {
                    for code_bit in code.iter_mut().rev() {
                        *code_bit ^= 1;
                        if *code_bit == 1 {
                            break;
                        }
                    }
                    code.resize(code_length.into(), 0);
                    symbols.push((symbol, code.clone()));
                }
                symbols
            }
            _ => vec![],
        }
    }
}

impl<E: Endianness, S: Clone + Default> Default for CanonicalHuffmanTree<E, S> {
    fn default() -> Self {
        Self::from_symbols(vec![(S::default(), vec![])]).unwrap_or_else(|_| unreachable!())
    }
}
