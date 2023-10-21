#![warn(missing_docs)]

//! `mediasan-common` is a common library shared by the `mediasan` media format "sanitizers".

#[macro_use]
pub mod macros;

pub mod async_skip;
pub mod error;
pub mod parse;
mod skip;
pub mod sync;
pub mod util;

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use derive_more::{Deref, DerefMut};

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

/// An adapter implementing [`Skip`]/[`AsyncSkip`] for all types implementing [`Seek`]/[`AsyncSeek`].
#[derive(Clone, Copy, Debug, Default, Deref, DerefMut)]
pub struct SeekSkipAdapter<T: ?Sized>(pub T);

pub use async_skip::AsyncSkipExt;
