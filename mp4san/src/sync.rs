use std::io;
use std::io::Read;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Buf;
use futures::{AsyncRead, FutureExt};

use crate::{sanitize_async_with_config, AsyncSkip, Config, Error, SanitizedMetadata, Skip};

//
// public functions
//

pub fn sanitize_with_config<R: Read + Skip + Unpin>(input: R, config: Config) -> Result<SanitizedMetadata, Error> {
    // Using AsyncInputAdapter is OK here because this is a blocking (non-async) API.
    let future = sanitize_async_with_config(AsyncInputAdapter(input), config);

    // `future` should never yield, as the wrapped synchronous input is the only thing `awaited` upon in the sanitizer.
    future.now_or_never().unwrap_or_else(|| unreachable!())
}

pub fn buf_async_reader<B: Buf + Unpin>(input: B) -> impl AsyncRead + Unpin {
    AsyncInputAdapter(input.reader())
}

//
// private types
//

/// An adapter for [`Read`] + [`Skip`] types implementing [`AsyncRead`] + [`AsyncSkip`].
///
/// The [`AsyncRead`] + [`AsyncSkip`] implementations will block on IO, so it must not be used when exposing async APIs.
struct AsyncInputAdapter<T>(T);

//
// AsyncInputAdapter impls
//

impl<T: Read + Unpin> AsyncRead for AsyncInputAdapter<T> {
    fn poll_read(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        self.0.read(buf).into()
    }

    fn poll_read_vectored(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        bufs: &mut [io::IoSliceMut<'_>],
    ) -> Poll<io::Result<usize>> {
        self.0.read_vectored(bufs).into()
    }
}

impl<T: Skip + Unpin> AsyncSkip for AsyncInputAdapter<T> {
    fn poll_skip(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, amount: u64) -> Poll<io::Result<()>> {
        self.0.skip(amount).into()
    }

    fn poll_stream_position(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        self.0.stream_position().into()
    }

    fn poll_stream_len(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        self.0.stream_len().into()
    }
}

impl<T: Unpin> Unpin for AsyncInputAdapter<T> {}
