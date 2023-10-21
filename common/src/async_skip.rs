//! Utility functions for the [`AsyncSkip`] trait.

use std::future::Future;
use std::io;
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use futures_util::io::{BufReader, Cursor};
use futures_util::{AsyncBufRead, AsyncRead, AsyncSeek};

use crate::{AsyncSkip, SeekSkipAdapter};

//
// public types
//

/// An extension trait which adds utility methods to [`AsyncSkip`] types.
pub trait AsyncSkipExt: AsyncSkip {
    /// Skip an amount of bytes in a stream.
    ///
    /// A skip beyond the end of a stream is allowed, but behavior is defined by the implementation.
    fn skip(&mut self, amount: u64) -> Skip<'_, Self> {
        Skip { amount, inner: self }
    }

    /// Returns the current position of the cursor from the start of the stream.
    fn stream_position(&mut self) -> StreamPosition<'_, Self> {
        StreamPosition { inner: self }
    }

    /// Returns the length of this stream, in bytes.
    fn stream_len(&mut self) -> StreamLen<'_, Self> {
        StreamLen { inner: self }
    }
}

/// Future for the [`skip`](AsyncSkipExt::skip) method.
pub struct Skip<'a, T: ?Sized> {
    amount: u64,
    inner: &'a mut T,
}

/// Future for the [`stream_position`](AsyncSkipExt::stream_position) method.
pub struct StreamPosition<'a, T: ?Sized> {
    inner: &'a mut T,
}

/// Future for the [`stream_len`](AsyncSkipExt::stream_len) method.
pub struct StreamLen<'a, T: ?Sized> {
    inner: &'a mut T,
}

//
// AsyncSkipExt impls
//

impl<T: AsyncSkip + ?Sized> AsyncSkipExt for T {}

//
// Skip impls
//

impl<T: AsyncSkip + Unpin + ?Sized> Future for Skip<'_, T> {
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let amount = self.amount;
        Pin::new(&mut *self.inner).poll_skip(cx, amount)
    }
}

//
// StreamPosition impls
//

impl<T: AsyncSkip + Unpin + ?Sized> Future for StreamPosition<'_, T> {
    type Output = io::Result<u64>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut *self.inner).poll_stream_position(cx)
    }
}

//
// StreamLen impls
//

impl<T: AsyncSkip + Unpin + ?Sized> Future for StreamLen<'_, T> {
    type Output = io::Result<u64>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut *self.inner).poll_stream_len(cx)
    }
}

//
// SeekSkipAdapter impls
//

impl<T: AsyncRead + Unpin + ?Sized> AsyncRead for SeekSkipAdapter<T> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl<R: AsyncSeek + Unpin + ?Sized> AsyncSkip for SeekSkipAdapter<R> {
    fn poll_skip(mut self: Pin<&mut Self>, cx: &mut Context<'_>, amount: u64) -> Poll<io::Result<()>> {
        match amount.try_into() {
            Ok(0) => (),
            Ok(amount) => {
                let reader = Pin::new(&mut self.get_mut().0);
                ready!(reader.poll_seek(cx, io::SeekFrom::Current(amount)))?;
            }
            Err(_) => {
                let stream_pos = ready!(self.as_mut().poll_stream_position(cx))?;
                let seek_pos = stream_pos
                    .checked_add(amount)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "seek past u64::MAX"))?;
                let reader = Pin::new(&mut self.get_mut().0);
                ready!(reader.poll_seek(cx, io::SeekFrom::Start(seek_pos)))?;
            }
        }
        Ok(()).into()
    }

    fn poll_stream_position(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        let reader = Pin::new(&mut self.get_mut().0);
        reader.poll_seek(cx, io::SeekFrom::Current(0))
    }

    fn poll_stream_len(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        // This is the unstable Seek::stream_len
        let stream_pos = ready!(self.as_mut().poll_stream_position(cx))?;
        let mut reader = Pin::new(&mut self.get_mut().0);
        let len = ready!(reader.as_mut().poll_seek(cx, io::SeekFrom::End(0)))?;

        if stream_pos != len {
            ready!(reader.poll_seek(cx, io::SeekFrom::Start(stream_pos)))?;
        }

        Ok(len).into()
    }
}

//
// AsyncSkip impls
//

macro_rules! deref_async_skip {
    () => {
        fn poll_skip(mut self: Pin<&mut Self>, cx: &mut Context<'_>, amount: u64) -> Poll<io::Result<()>> {
            Pin::new(&mut **self).poll_skip(cx, amount)
        }

        fn poll_stream_position(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
            Pin::new(&mut **self).poll_stream_position(cx)
        }

        fn poll_stream_len(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
            Pin::new(&mut **self).poll_stream_len(cx)
        }
    };
}

impl<R: AsyncSkip + Unpin + ?Sized> AsyncSkip for &mut R {
    deref_async_skip!();
}

impl<R: AsyncSkip + Unpin + ?Sized> AsyncSkip for Box<R> {
    deref_async_skip!();
}

impl<P: DerefMut + Unpin> AsyncSkip for Pin<P>
where
    P::Target: AsyncSkip,
{
    fn poll_skip(self: Pin<&mut Self>, cx: &mut Context<'_>, amount: u64) -> Poll<io::Result<()>> {
        self.get_mut().as_mut().poll_skip(cx, amount)
    }

    fn poll_stream_position(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        self.get_mut().as_mut().poll_stream_position(cx)
    }

    fn poll_stream_len(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        self.get_mut().as_mut().poll_stream_len(cx)
    }
}

macro_rules! async_skip_via_adapter {
    () => {
        fn poll_skip(self: Pin<&mut Self>, cx: &mut Context<'_>, amount: u64) -> Poll<io::Result<()>> {
            Pin::new(&mut SeekSkipAdapter(self.get_mut())).poll_skip(cx, amount)
        }

        fn poll_stream_position(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
            Pin::new(&mut SeekSkipAdapter(self.get_mut())).poll_stream_position(cx)
        }

        fn poll_stream_len(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
            Pin::new(&mut SeekSkipAdapter(self.get_mut())).poll_stream_len(cx)
        }
    };
}

impl<T: AsRef<[u8]> + Unpin> AsyncSkip for Cursor<T> {
    async_skip_via_adapter!();
}

impl<R: AsyncRead + AsyncSkip> AsyncSkip for BufReader<R> {
    /// Poll skipping `amount` bytes in a [`BufReader`] implementing [`AsyncRead`] + [`AsyncSkip`].
    fn poll_skip(mut self: Pin<&mut Self>, cx: &mut Context<'_>, amount: u64) -> Poll<io::Result<()>> {
        let buf_len = self.buffer().len();
        if let Some(skip_amount) = amount.checked_sub(buf_len as u64) {
            if skip_amount != 0 {
                ready!(self.as_mut().get_pin_mut().poll_skip(cx, skip_amount))?
            }
        }
        self.consume(buf_len.min(amount as usize));
        Poll::Ready(Ok(()))
    }

    /// Poll the stream position for a [`BufReader`] implementing [`AsyncRead`] + [`AsyncSkip`].
    fn poll_stream_position(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        let stream_pos = ready!(self.as_mut().get_pin_mut().poll_stream_position(cx))?;
        Poll::Ready(Ok(stream_pos.saturating_sub(self.buffer().len() as u64)))
    }

    /// Poll the stream length for a [`BufReader`] implementing [`AsyncRead`] + [`AsyncSkip`].
    fn poll_stream_len(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        self.as_mut().get_pin_mut().poll_stream_len(cx)
    }
}
