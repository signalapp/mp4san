#![warn(missing_docs)]

//! `mediasan-common` is a common library shared by the `mediasan` media format "sanitizers".

#[macro_use]
pub mod macros;

pub mod async_skip;
pub mod error;
pub mod parse;
pub mod skip;
pub mod sync;
pub mod util;

use std::io;
use std::io::Seek;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use futures_util::AsyncSeek;

//
// public types
//

pub use error::{Error, Report, Result, ResultExt};

/// A pointer to a span in the given input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InputSpan {
    /// The offset from the beginning of the input where the span begins.
    pub offset: u64,

    /// The length of the span.
    pub len: u64,
}

/// A subset of the [`Seek`] trait, providing a cursor which can skip forward within a stream of bytes.
///
/// [`Skip`] is automatically implemented for all types implementing [`Seek`].
pub trait Skip {
    /// Skip an amount of bytes in a stream.
    ///
    /// A skip beyond the end of a stream is allowed, but behavior is defined by the implementation.
    fn skip(&mut self, amount: u64) -> io::Result<()>;

    /// Returns the current position of the cursor from the start of the stream.
    fn stream_position(&mut self) -> io::Result<u64>;

    /// Returns the length of this stream, in bytes.
    fn stream_len(&mut self) -> io::Result<u64>;
}

/// A subset of the [`AsyncSeek`] trait, providing a cursor which can skip forward within a stream of bytes.
///
/// [`AsyncSkip`] is automatically implemented for all types implementing [`AsyncSeek`].
pub trait AsyncSkip {
    /// Skip an amount of bytes in a stream.
    ///
    /// A skip beyond the end of a stream is allowed, but behavior is defined by the implementation.
    fn poll_skip(self: Pin<&mut Self>, cx: &mut Context<'_>, amount: u64) -> Poll<io::Result<()>>;

    /// Returns the current position of the cursor from the start of the stream.
    fn poll_stream_position(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>>;

    /// Returns the length of this stream, in bytes.
    fn poll_stream_len(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>>;
}

//
// public functions
//

//
// Skip impls
//

impl<T: Seek> Skip for T {
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        match amount.try_into() {
            Ok(0) => (),
            Ok(amount) => {
                self.seek(io::SeekFrom::Current(amount))?;
            }
            Err(_) => {
                let stream_pos = self.stream_position()?;
                let seek_pos = stream_pos
                    .checked_add(amount)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "seek past u64::MAX"))?;
                self.seek(io::SeekFrom::Start(seek_pos))?;
            }
        }
        Ok(())
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        io::Seek::stream_position(self)
    }

    fn stream_len(&mut self) -> io::Result<u64> {
        // This is the unstable Seek::stream_len
        let stream_pos = self.stream_position()?;
        let len = self.seek(io::SeekFrom::End(0))?;

        if stream_pos != len {
            self.seek(io::SeekFrom::Start(stream_pos))?;
        }

        Ok(len)
    }
}

//
// AsyncSkip impls
//

impl<T: AsyncSeek> AsyncSkip for T {
    fn poll_skip(mut self: Pin<&mut Self>, cx: &mut Context<'_>, amount: u64) -> Poll<io::Result<()>> {
        match amount.try_into() {
            Ok(0) => (),
            Ok(amount) => {
                ready!(self.poll_seek(cx, io::SeekFrom::Current(amount)))?;
            }
            Err(_) => {
                let stream_pos = ready!(self.as_mut().poll_stream_position(cx))?;
                let seek_pos = stream_pos
                    .checked_add(amount)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "seek past u64::MAX"))?;
                ready!(self.poll_seek(cx, io::SeekFrom::Start(seek_pos)))?;
            }
        }
        Ok(()).into()
    }

    fn poll_stream_position(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        self.poll_seek(cx, io::SeekFrom::Current(0))
    }

    fn poll_stream_len(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        // This is the unstable Seek::stream_len
        let stream_pos = ready!(self.as_mut().poll_stream_position(cx))?;
        let len = ready!(self.as_mut().poll_seek(cx, io::SeekFrom::End(0)))?;

        if stream_pos != len {
            ready!(self.poll_seek(cx, io::SeekFrom::Start(stream_pos)))?;
        }

        Ok(len).into()
    }
}
