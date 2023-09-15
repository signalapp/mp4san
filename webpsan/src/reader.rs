use core::slice;
use std::io;
use std::num::NonZeroU32;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use bytes::BytesMut;
use futures_util::io::BufReader;
use futures_util::{AsyncBufReadExt, AsyncRead, AsyncReadExt};
use mediasan_common::error::{ExtraUnparsedInput, WhileParsingType};
use mediasan_common::parse::FourCC;
use mediasan_common::util::IoResultExt;
use mediasan_common::{
    bail_attach, buf_skip, buf_stream_position, ensure_attach, ensure_matches_attach, poll_buf_skip,
    poll_buf_stream_len, poll_buf_stream_position, report_attach, AsyncSkip, InputSpan,
};
use pin_project::pin_project;

use crate::parse::error::{ExpectedChunk, ParseResultExt, WhileParsingChunk};
use crate::parse::{ChunkHeader, ParseChunk, ParseError, WebmPrim};
use crate::Error;

#[pin_project]
pub struct ChunkReader<R> {
    state: State,
    #[pin]
    inner: BufReader<R>,
}

pub struct ChunkDataReader<'a, R> {
    reader: Pin<&'a mut ChunkReader<R>>,
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

impl<R: AsyncRead + AsyncSkip> ChunkReader<R> {
    pub fn new(input: R, chunk_name: FourCC) -> Self {
        let inner = BufReader::with_capacity(ChunkHeader::ENCODED_LEN as usize, input);
        Self { state: State::Idle { last: chunk_name }, inner }
    }

    pub async fn has_remaining(self: &mut Pin<&mut Self>) -> Result<bool, Error> {
        match self.read_padding().await? {
            State::Idle { .. } => (),
            State::PeekingHeader { .. } => return Ok(true),
            State::ReadingBody { .. } => return Ok(true),
            State::ReadingPadding { token, .. } => match token {},
        }
        Ok(!self.as_mut().project().inner.fill_buf().await?.is_empty())
    }

    /// Read a chunk header, also saving it to be returned by [`read_header`](Self::read_header) later.
    pub async fn peek_header(self: &mut Pin<&mut Self>) -> Result<Option<FourCC>, Error> {
        let header = match self.read_padding().await? {
            State::PeekingHeader { header } => header,
            State::Idle { .. } => {
                if !self.has_remaining().await? {
                    return Ok(None);
                }
                ChunkHeader::read(self.as_mut().project().inner).await.map_eof(|_| {
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

        *self.as_mut().project().state = State::PeekingHeader { header };
        Ok(Some(header.name))
    }

    /// Read a specific chunk header.
    pub async fn read_header(self: &mut Pin<&mut Self>, name: FourCC) -> Result<InputSpan, Error> {
        match self.read_padding().await? {
            State::Idle { .. } => ensure_attach!(self.has_remaining().await?, ParseError::MissingRequiredChunk(name)),
            State::PeekingHeader { .. } | State::ReadingBody { .. } => (),
            State::ReadingPadding { token, .. } => match token {},
        }
        let (read_name, span) = self.read_any_header().await?;
        ensure_attach!(
            read_name == name,
            ParseError::InvalidChunkLayout,
            ExpectedChunk(name),
            WhileParsingChunk(read_name),
        );
        Ok(span)
    }

    /// Read a chunk header.
    pub async fn read_any_header(self: &mut Pin<&mut Self>) -> Result<(FourCC, InputSpan), Error> {
        let header = match self.read_padding().await? {
            State::PeekingHeader { header } => header,
            State::Idle { .. } => {
                ensure_attach!(self.has_remaining().await?, ParseError::InvalidChunkLayout);
                ChunkHeader::read(self.as_mut().project().inner).await.map_eof(|_| {
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

        *self.as_mut().project().state = match NonZeroU32::new(header.len) {
            Some(remaining) => State::ReadingBody { header, remaining },
            None => State::ReadingPadding { header, token: () },
        };

        let offset = buf_stream_position(self.as_mut().project().inner).await? - u64::from(ChunkHeader::ENCODED_LEN);
        let len = u64::from(header.len) + u64::from(ChunkHeader::ENCODED_LEN);
        Ok((header.name, InputSpan { offset, len }))
    }

    /// Read and parse a chunks's data assuming its header has already been read.
    pub async fn parse_data<T: ParseChunk>(self: &mut Pin<&mut Self>) -> Result<T, Error> {
        let mut data = self.read_data(T::ENCODED_LEN).await?;
        let parsed = T::parse(&mut data).while_parsing_chunk(self.current_chunk_name())?;
        Ok(parsed)
    }

    /// Read `len` of a chunks's data assuming its header has already been read.
    pub async fn read_data(self: &mut Pin<&mut Self>, len: u32) -> Result<BytesMut, Error> {
        let (header, remaining) = match self.read_padding().await? {
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
        self.as_mut().project().inner.read_exact(&mut data).await.map_eof(|_| {
            Error::Parse(report_attach!(
                ParseError::TruncatedChunk,
                WhileParsingChunk(header.name),
            ))
        })?;

        *self.as_mut().project().state = match NonZeroU32::new(new_remaining) {
            Some(remaining) => State::ReadingBody { header, remaining },
            None => State::ReadingPadding { header, token: () },
        };

        Ok(data)
    }

    /// Skip a chunks's data assuming its header has already been read.
    pub async fn skip_data(self: &mut Pin<&mut Self>) -> Result<(), Error> {
        let (header, remaining) = match self.read_padding().await? {
            State::Idle { .. } => return Ok(()),
            State::PeekingHeader { header: ChunkHeader { name, .. } } => {
                panic!("read_header must be read after peek_header for {name}");
            }
            State::ReadingBody { header, remaining } => (header, remaining),
            State::ReadingPadding { token, .. } => match token {},
        };

        buf_skip(self.as_mut().project().inner, remaining.get().into())
            .await
            .map_eof(|_| {
                Error::Parse(report_attach!(
                    ParseError::TruncatedChunk,
                    WhileParsingChunk(header.name),
                ))
            })?;

        *self.as_mut().project().state = State::ReadingPadding { header, token: () };

        Ok(())
    }

    /// Return an [`AsyncRead`] + [`AsyncSkip`] type over a chunk's data, assuming its header has already been read.
    pub fn data_reader<'a>(self: &'a mut Pin<&mut Self>) -> ChunkDataReader<'a, R> {
        ChunkDataReader { reader: self.as_mut() }
    }

    /// Return a [`ChunkReader`] type over a chunk's data, assuming its header has already been read.
    pub fn child_reader<'a>(self: &'a mut Pin<&mut Self>) -> ChunkReader<ChunkDataReader<'a, R>> {
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

    async fn read_padding(self: &mut Pin<&mut Self>) -> Result<State<PaddingReadToken>, Error> {
        let header = match self.state {
            State::Idle { last } => return Ok(State::Idle { last }),
            State::PeekingHeader { header } => return Ok(State::PeekingHeader { header }),
            State::ReadingBody { header, remaining } => return Ok(State::ReadingBody { header, remaining }),
            State::ReadingPadding { header, token: () } => header,
        };

        if header.padded() {
            let mut pad = 0;
            let mut inner = self.as_mut().project().inner;
            inner.read_exact(slice::from_mut(&mut pad)).await.map_eof(|_| {
                Error::Parse(report_attach!(
                    ParseError::TruncatedChunk,
                    WhileParsingChunk(header.name),
                ))
            })?;
            ensure_matches_attach!(pad, 0, ParseError::InvalidInput, WhileParsingChunk(header.name));
        }

        *self.as_mut().project().state = State::Idle { last: header.name };

        Ok(State::Idle { last: header.name })
    }
}

//
// ChunkDataReader impls
//

impl<R: AsyncRead> AsyncRead for ChunkDataReader<'_, R> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        let reader = self.reader.as_mut().project();

        let (header, remaining) = match &*reader.state {
            State::Idle { .. } | State::ReadingPadding { .. } => return Poll::Ready(Ok(0)),
            State::PeekingHeader { header: ChunkHeader { name, .. } } => {
                panic!("read_header must be read after peek_header for {name}");
            }
            &State::ReadingBody { header, remaining } => (header, remaining),
        };

        let read_len = buf.len().min(remaining.get() as usize);
        let amount_read = ready!(reader.inner.poll_read(cx, &mut buf[..read_len]))?;
        *reader.state = match NonZeroU32::new(remaining.get() - amount_read as u32) {
            Some(remaining) => State::ReadingBody { header, remaining },
            None => State::ReadingPadding { header, token: () },
        };
        Poll::Ready(Ok(amount_read))
    }
}

impl<R: AsyncRead + AsyncSkip> AsyncSkip for ChunkDataReader<'_, R> {
    fn poll_skip(mut self: Pin<&mut Self>, cx: &mut Context<'_>, skip_amount: u64) -> Poll<io::Result<()>> {
        let mut reader = self.reader.as_mut().project();
        let (header, remaining) = match &*reader.state {
            State::Idle { .. } | State::ReadingPadding { .. } if skip_amount == 0 => return Poll::Ready(Ok(())),
            State::Idle { .. } | State::ReadingPadding { .. } => {
                return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()))
            }
            State::PeekingHeader { header: ChunkHeader { name, .. } } => {
                panic!("read_header must be read after peek_header for {name}");
            }
            &State::ReadingBody { header, remaining } => (header, remaining),
        };

        if skip_amount > remaining.get().into() {
            return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
        }

        ready!(poll_buf_skip(reader.inner.as_mut(), cx, skip_amount))?;

        *reader.state = match NonZeroU32::new(remaining.get() - skip_amount as u32) {
            Some(remaining) => State::ReadingBody { header, remaining },
            None => State::ReadingPadding { header, token: () },
        };
        Poll::Ready(Ok(()))
    }

    fn poll_stream_position(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        poll_buf_stream_position(self.reader.as_mut().project().inner.as_mut(), cx)
    }

    fn poll_stream_len(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        poll_buf_stream_len(self.reader.as_mut().project().inner.as_mut(), cx)
    }
}
