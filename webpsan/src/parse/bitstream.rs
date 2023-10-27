#![allow(missing_docs)]

use std::fmt::Debug;
use std::io::{Cursor, Read};
use std::mem::replace;
use std::num::NonZeroU32;

use bitstream_io::huffman::{compile_read_tree, ReadHuffmanTree};
use bitstream_io::{BitRead, BitReader, Endianness, HuffmanRead, Numeric};
use derive_more::Display;
use mediasan_common::util::IoResultExt;
use mediasan_common::{bail_attach, report_attach};

use crate::parse::ParseError;
use crate::Error;

pub struct BitBufReader<R, E: Endianness> {
    input: Option<R>,
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

pub const LZ77_MAX_LEN: u16 = (LZ77_MAX_SYMBOL - 2) >> 1;

const LZ77_MAX_SYMBOL: u16 = 39;

//
// BitBufReader impls
//

impl<R: Read, E: Endianness> BitBufReader<R, E> {
    pub fn with_capacity(input: R, capacity: usize) -> Self {
        Self { input: Some(input), reader: BitReader::new(Cursor::new(Vec::with_capacity(capacity))), buf_len: 0 }
    }

    pub fn fill_buf(&mut self) -> Result<(), Error> {
        let bit_pos = self.buf_bit_pos();
        let byte_pos = (bit_pos / 8) as usize;

        let Some(input) = self.input.as_mut() else {
            return Ok(());
        };

        let reader = replace(&mut self.reader, BitReader::new(Cursor::new(Vec::new())));
        let mut buf = reader.into_reader().into_inner();

        buf.drain(..byte_pos);
        input.take((buf.capacity() - buf.len()) as u64).read_to_end(&mut buf)?;
        if self.buf_len - byte_pos == buf.len() {
            self.input = None;
        }
        self.buf_len = buf.len();

        self.reader = BitReader::new(Cursor::new(buf));
        self.reader.skip((bit_pos % 8) as u32)?;
        Ok(())
    }

    pub fn buf_bits(&mut self) -> u64 {
        self.buf_len as u64 * 8 - self.buf_bit_pos()
    }

    pub fn buf_read<T: Numeric>(&mut self, bits: u32) -> Result<T, Error> {
        self.reader
            .read(bits)
            .map_eof(|_| Error::Parse(report_attach!(ParseError::TruncatedChunk)))
    }

    pub fn buf_read_bit(&mut self) -> Result<bool, Error> {
        self.reader
            .read_bit()
            .map_eof(|_| Error::Parse(report_attach!(ParseError::TruncatedChunk)))
    }

    pub fn buf_read_huffman<T: Clone>(&mut self, tree: &CanonicalHuffmanTree<E, T>) -> Result<T, Error> {
        self.reader
            .read_huffman(&tree.read_tree)
            .map_eof(|_| Error::Parse(report_attach!(ParseError::TruncatedChunk)))
    }

    pub fn buf_read_lz77(&mut self, prefix_code: u16) -> Result<NonZeroU32, Error> {
        match prefix_code {
            0..=3 => Ok(NonZeroU32::MIN.saturating_add(prefix_code.into())),
            4..=LZ77_MAX_SYMBOL => {
                let extra_bits = (u32::from(prefix_code) - 2) >> 1;
                let offset = (2 + (u32::from(prefix_code) & 1)) << extra_bits;
                Ok(NonZeroU32::MIN.saturating_add(offset + self.buf_read::<u32>(extra_bits)?))
            }
            _ => bail_attach!(ParseError::InvalidInput, InvalidLz77PrefixCode(prefix_code)),
        }
    }

    pub fn read<T: Numeric>(&mut self, bits: u32) -> Result<T, Error> {
        if self.buf_bits() < bits.into() {
            self.fill_buf()?;
        }
        self.buf_read(bits)
    }

    pub fn read_bit(&mut self) -> Result<bool, Error> {
        if self.buf_bits() < 1 {
            self.fill_buf()?;
        }
        self.buf_read_bit()
    }

    pub fn read_huffman<T: Clone>(&mut self, tree: &CanonicalHuffmanTree<E, T>) -> Result<T, Error> {
        if self.buf_bits() < tree.longest_code_len.into() {
            self.fill_buf()?;
        }
        self.buf_read_huffman(tree)
    }

    fn buf_bit_pos(&mut self) -> u64 {
        self.reader.position_in_bits().unwrap_or_else(|_| unreachable!())
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
        let symbols = Self::symbols(code_lengths);
        log::debug!("symbols: {symbols:?}");
        Self::from_symbols(symbols)
    }

    pub fn from_symbols(symbols: Vec<(S, Vec<u8>)>) -> Result<Self, Error> {
        let longest_code_len = match &symbols[..] {
            [_symbol] => 0,
            _ => symbols.iter().map(|(_, code)| code.len()).max().unwrap_or_default() as u32,
        };
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
