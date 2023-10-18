//! Utility functions for the [`AsyncSkip`] trait.

use std::future::poll_fn;
use std::io;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use futures_util::io::BufReader;
use futures_util::{AsyncBufRead, AsyncRead};

use crate::AsyncSkip;

/// Poll skipping `amount` bytes in a [`BufReader`] implementing [`AsyncRead`] + [`AsyncSkip`].
pub fn poll_buf_skip<R: AsyncRead + AsyncSkip>(
    mut reader: Pin<&mut BufReader<R>>,
    cx: &mut Context<'_>,
    amount: u64,
) -> Poll<io::Result<()>> {
    let buf_len = reader.buffer().len();
    if let Some(skip_amount) = amount.checked_sub(buf_len as u64) {
        if skip_amount != 0 {
            ready!(reader.as_mut().get_pin_mut().poll_skip(cx, skip_amount))?
        }
    }
    reader.consume(buf_len.min(amount as usize));
    Poll::Ready(Ok(()))
}

/// Skip `amount` bytes in a [`BufReader`] implementing [`AsyncRead`] + [`AsyncSkip`].
pub async fn buf_skip<R: AsyncRead + AsyncSkip>(mut reader: Pin<&mut BufReader<R>>, amount: u64) -> io::Result<()> {
    poll_fn(|cx| poll_buf_skip(reader.as_mut(), cx, amount)).await
}

/// Poll the stream position for a [`BufReader`] implementing [`AsyncRead`] + [`AsyncSkip`].
pub fn poll_buf_stream_position<R: AsyncRead + AsyncSkip>(
    mut reader: Pin<&mut BufReader<R>>,
    cx: &mut Context<'_>,
) -> Poll<io::Result<u64>> {
    let stream_pos = ready!(reader.as_mut().get_pin_mut().poll_stream_position(cx))?;
    Poll::Ready(Ok(stream_pos.saturating_sub(reader.buffer().len() as u64)))
}

/// Return the stream position for a [`BufReader`] implementing [`AsyncRead`] + [`AsyncSkip`].
pub async fn buf_stream_position<R: AsyncRead + AsyncSkip>(mut reader: Pin<&mut BufReader<R>>) -> io::Result<u64> {
    poll_fn(|cx| poll_buf_stream_position(reader.as_mut(), cx)).await
}

/// Poll the stream length for a [`BufReader`] implementing [`AsyncRead`] + [`AsyncSkip`].
pub fn poll_buf_stream_len<R: AsyncRead + AsyncSkip>(
    mut reader: Pin<&mut BufReader<R>>,
    cx: &mut Context<'_>,
) -> Poll<io::Result<u64>> {
    reader.as_mut().get_pin_mut().poll_stream_len(cx)
}

/// Return the stream length for a [`BufReader`] implementing [`AsyncRead`] + [`AsyncSkip`].
pub async fn buf_stream_len<R: AsyncRead + AsyncSkip>(mut reader: Pin<&mut BufReader<R>>) -> io::Result<u64> {
    poll_fn(|cx| poll_buf_stream_len(reader.as_mut(), cx)).await
}
