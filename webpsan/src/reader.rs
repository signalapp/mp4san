use core::slice;
use std::io;
use std::io::{BufRead, BufReader, Read};
use std::num::NonZeroU32;

use bytes::BytesMut;
use mediasan_common::error::{ExtraUnparsedInput, WhileParsingType};
use mediasan_common::parse::FourCC;
use mediasan_common::skip::{buf_skip, buf_stream_len, buf_stream_position};
use mediasan_common::util::IoResultExt;
use mediasan_common::{bail_attach, ensure_attach, ensure_matches_attach, report_attach, InputSpan, Skip};

use crate::parse::error::{ExpectedChunk, ParseResultExt, WhileParsingChunk};
use crate::parse::{ChunkHeader, ParseChunk, ParseError, WebmPrim};
use crate::Error;

pub struct ChunkReader<R> {
    state: State,
    inner: BufReader<R>,
}

pub struct ChunkDataReader<'a, R> {
    reader: &'a mut ChunkReader<R>,
}

enum State<P = ()> {
    Idle { last: FourCC },
    PeekingHeader { header: ChunkHeader },
    ReadingBody { header: ChunkHeader, remaining: NonZeroU32 },
    ReadingPadding { header: ChunkHeader, token: P },
}

enum PaddingReadToken {}

//
// ChunkReader impls
//

impl<R: Read + Skip> ChunkReader<R> {
    pub fn new(input: R, chunk_name: FourCC) -> Self {
        let inner = BufReader::with_capacity(ChunkHeader::ENCODED_LEN as usize, input);
        Self { state: State::Idle { last: chunk_name }, inner }
    }

    pub fn has_remaining(&mut self) -> Result<bool, Error> {
        match self.read_padding()? {
            State::Idle { .. } => (),
            State::PeekingHeader { .. } => return Ok(true),
            State::ReadingBody { .. } => return Ok(true),
            State::ReadingPadding { token, .. } => match token {},
        }
        Ok(!self.inner.fill_buf()?.is_empty())
    }

    /// Read a chunk header, also saving it to be returned by [`read_header`](Self::read_header) later.
    pub fn peek_header(&mut self) -> Result<Option<FourCC>, Error> {
        let header = match self.read_padding()? {
            State::PeekingHeader { header } => header,
            State::Idle { .. } => {
                if !self.has_remaining()? {
                    return Ok(None);
                }
                ChunkHeader::read(&mut self.inner).map_eof(|_| {
                    Error::Parse(report_attach!(
                        ParseError::TruncatedChunk,
                        WhileParsingType::new::<ChunkHeader>(),
                    ))
                })?
            }
            State::ReadingBody { header, .. } => bail_attach!(
                ParseError::InvalidInput,
                ExtraUnparsedInput,
                WhileParsingChunk(header.name)
            ),
            State::ReadingPadding { token, .. } => match token {},
        };

        self.state = State::PeekingHeader { header };
        Ok(Some(header.name))
    }

    /// Read a specific chunk header.
    pub fn read_header(&mut self, name: FourCC) -> Result<InputSpan, Error> {
        match self.read_padding()? {
            State::Idle { .. } => ensure_attach!(self.has_remaining()?, ParseError::MissingRequiredChunk(name)),
            State::PeekingHeader { .. } | State::ReadingBody { .. } => (),
            State::ReadingPadding { token, .. } => match token {},
        }
        let (read_name, span) = self.read_any_header()?;
        ensure_attach!(
            read_name == name,
            ParseError::InvalidChunkLayout,
            ExpectedChunk(name),
            WhileParsingChunk(read_name),
        );
        Ok(span)
    }

    /// Read a chunk header.
    pub fn read_any_header(&mut self) -> Result<(FourCC, InputSpan), Error> {
        let header = match self.read_padding()? {
            State::PeekingHeader { header } => header,
            State::Idle { .. } => {
                ensure_attach!(self.has_remaining()?, ParseError::InvalidChunkLayout);
                ChunkHeader::read(&mut self.inner).map_eof(|_| {
                    Error::Parse(report_attach!(
                        ParseError::TruncatedChunk,
                        WhileParsingType::new::<ChunkHeader>(),
                    ))
                })?
            }
            State::ReadingBody { header, .. } => bail_attach!(
                ParseError::InvalidInput,
                WhileParsingType::new::<ChunkHeader>(),
                ExtraUnparsedInput,
                WhileParsingChunk(header.name),
            ),
            State::ReadingPadding { token, .. } => match token {},
        };

        self.state = match NonZeroU32::new(header.len) {
            Some(remaining) => State::ReadingBody { header, remaining },
            None => State::ReadingPadding { header, token: () },
        };

        let offset = buf_stream_position(&mut self.inner)? - u64::from(ChunkHeader::ENCODED_LEN);
        let len = u64::from(header.len) + u64::from(ChunkHeader::ENCODED_LEN);
        Ok((header.name, InputSpan { offset, len }))
    }

    /// Read and parse a chunks's data assuming its header has already been read.
    pub fn parse_data<T: ParseChunk>(&mut self) -> Result<T, Error> {
        let mut data = self.read_data(T::ENCODED_LEN)?;
        let parsed = T::parse(&mut data).while_parsing_chunk(self.current_chunk_name())?;
        Ok(parsed)
    }

    /// Read `len` of a chunks's data assuming its header has already been read.
    pub fn read_data(&mut self, len: u32) -> Result<BytesMut, Error> {
        let (header, remaining) = match self.read_padding()? {
            State::Idle { last } => bail_attach!(ParseError::TruncatedChunk, WhileParsingChunk(last)),
            State::PeekingHeader { header: ChunkHeader { name, .. } } => {
                panic!("read_header must be read after peek_header for {name}");
            }
            State::ReadingBody { header, remaining } => (header, remaining),
            State::ReadingPadding { token, .. } => match token {},
        };

        ensure_matches_attach!(
            remaining.get().checked_sub(len),
            Some(new_remaining),
            ParseError::TruncatedChunk,
            WhileParsingChunk(header.name)
        );

        let mut data = BytesMut::zeroed(len as usize);
        self.inner.read_exact(&mut data).map_eof(|_| {
            Error::Parse(report_attach!(
                ParseError::TruncatedChunk,
                WhileParsingChunk(header.name),
            ))
        })?;

        self.state = match NonZeroU32::new(new_remaining) {
            Some(remaining) => State::ReadingBody { header, remaining },
            None => State::ReadingPadding { header, token: () },
        };

        Ok(data)
    }

    /// Skip a chunks's data assuming its header has already been read.
    pub fn skip_data(&mut self) -> Result<(), Error> {
        let (header, remaining) = match self.read_padding()? {
            State::Idle { .. } => return Ok(()),
            State::PeekingHeader { header: ChunkHeader { name, .. } } => {
                panic!("read_header must be read after peek_header for {name}");
            }
            State::ReadingBody { header, remaining } => (header, remaining),
            State::ReadingPadding { token, .. } => match token {},
        };

        buf_skip(&mut self.inner, remaining.get().into()).map_eof(|_| {
            Error::Parse(report_attach!(
                ParseError::TruncatedChunk,
                WhileParsingChunk(header.name),
            ))
        })?;

        self.state = State::ReadingPadding { header, token: () };

        Ok(())
    }

    /// Return an [`AsyncRead`] + [`AsyncSkip`] type over a chunk's data, assuming its header has already been read.
    pub fn data_reader(&mut self) -> ChunkDataReader<'_, R> {
        ChunkDataReader { reader: self }
    }

    /// Return a [`ChunkReader`] type over a chunk's data, assuming its header has already been read.
    pub fn child_reader(&mut self) -> ChunkReader<ChunkDataReader<'_, R>> {
        let name = self.current_chunk_name();
        ChunkReader::new(self.data_reader(), name)
    }

    fn current_chunk_name(&self) -> FourCC {
        match &self.state {
            &State::Idle { last } => last,
            State::PeekingHeader { header, .. } => header.name,
            State::ReadingBody { header, .. } => header.name,
            State::ReadingPadding { header, .. } => header.name,
        }
    }

    fn read_padding(&mut self) -> Result<State<PaddingReadToken>, Error> {
        let header = match self.state {
            State::Idle { last } => return Ok(State::Idle { last }),
            State::PeekingHeader { header } => return Ok(State::PeekingHeader { header }),
            State::ReadingBody { header, remaining } => return Ok(State::ReadingBody { header, remaining }),
            State::ReadingPadding { header, token: () } => header,
        };

        if header.padded() {
            let mut pad = 0;
            self.inner.read_exact(slice::from_mut(&mut pad)).map_eof(|_| {
                Error::Parse(report_attach!(
                    ParseError::TruncatedChunk,
                    WhileParsingChunk(header.name),
                ))
            })?;
            ensure_matches_attach!(pad, 0, ParseError::InvalidInput, WhileParsingChunk(header.name));
        }

        self.state = State::Idle { last: header.name };

        Ok(State::Idle { last: header.name })
    }
}

//
// ChunkDataReader impls
//

impl<R: Read> Read for ChunkDataReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let (header, remaining) = match &self.reader.state {
            State::Idle { .. } | State::ReadingPadding { .. } => return Ok(0),
            State::PeekingHeader { header: ChunkHeader { name, .. } } => {
                panic!("read_header must be read after peek_header for {name}");
            }
            &State::ReadingBody { header, remaining } => (header, remaining),
        };

        let read_len = buf.len().min(remaining.get() as usize);
        let amount_read = self.reader.inner.read(&mut buf[..read_len])?;
        self.reader.state = match NonZeroU32::new(remaining.get() - amount_read as u32) {
            Some(remaining) => State::ReadingBody { header, remaining },
            None => State::ReadingPadding { header, token: () },
        };
        Ok(amount_read)
    }
}

impl<R: Read + Skip> Skip for ChunkDataReader<'_, R> {
    fn skip(&mut self, skip_amount: u64) -> io::Result<()> {
        let (header, remaining) = match &self.reader.state {
            State::Idle { .. } | State::ReadingPadding { .. } if skip_amount == 0 => return Ok(()),
            State::Idle { .. } | State::ReadingPadding { .. } => return Err(io::ErrorKind::UnexpectedEof.into()),
            State::PeekingHeader { header: ChunkHeader { name, .. } } => {
                panic!("read_header must be read after peek_header for {name}");
            }
            &State::ReadingBody { header, remaining } => (header, remaining),
        };

        if skip_amount > remaining.get().into() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }

        buf_skip(&mut self.reader.inner, skip_amount)?;

        self.reader.state = match NonZeroU32::new(remaining.get() - skip_amount as u32) {
            Some(remaining) => State::ReadingBody { header, remaining },
            None => State::ReadingPadding { header, token: () },
        };
        Ok(())
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        buf_stream_position(&mut self.reader.inner)
    }

    fn stream_len(&mut self) -> io::Result<u64> {
        buf_stream_len(&mut self.reader.inner)
    }
}
