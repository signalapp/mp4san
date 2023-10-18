//! Error types returned by the unstable parsing API.

use std::fmt::{Debug, Display};

use derive_more::Display;
use mediasan_common::error::ReportableError;
use mediasan_common::parse::FourCC;
use mediasan_common::{Result, ResultExt};

/// Error type returned by the WebP parser.
///
/// While the API of this error type is currently considered unstable, it is more stably guaranteed to implement
/// [`Display`] + [`Debug`].
#[derive(Clone, Debug, thiserror::Error)]
pub enum ParseError {
    /// The input is invalid because its chunks are in a ordering or configuration disallowed by the WebP specification.
    #[error("Invalid chunk layout")]
    InvalidChunkLayout,

    /// The input is invalid.
    #[error("Invalid input")]
    InvalidInput,

    /// The VP8L image data contained an invalid prefix code.
    #[error("Invalid VP8L prefix code")]
    InvalidVp8lPrefixCode,

    /// The input is invalid because it is missing a chunk required by the WebP specification.
    #[error("Missing required `{_0}` chunk")]
    MissingRequiredChunk(FourCC),

    /// The input is invalid because the input ended before the end of a valid RIFF chunk.
    ///
    /// This can occur either when the entire input is truncated or when a chunk size is incorrect.
    #[error("TruncatedChunk")]
    TruncatedChunk,

    /// The input is unsupported because it contains an unknown chunk type.
    #[error("Unsupported chunk `{_0}`")]
    UnsupportedChunk(FourCC),

    /// The input is unsupported because it contains an unknown VP8L stream version.
    #[error("Unsupported VP8L version `{_0}`")]
    UnsupportedVp8lVersion(u8),
}

pub(crate) trait ParseResultExt: ResultExt + Sized {
    fn while_parsing_chunk(self, chunk_type: FourCC) -> Self {
        self.attach_printable(WhileParsingChunk(chunk_type))
    }

    fn while_parsing_field<T>(self, chunk_type: FourCC, field_name: T) -> Self
    where
        T: Display + Debug + Send + Sync + 'static,
    {
        self.attach_printable(WhileParsingField(chunk_type, field_name))
    }
}

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "multiple `{}` chunks", _0)]
pub(crate) struct MultipleChunks(pub(crate) FourCC);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "expected `{}` chunk", _0)]
pub(crate) struct ExpectedChunk(pub(crate) FourCC);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "while parsing `{}` chunk", _0)]
pub(crate) struct WhileParsingChunk(pub(crate) FourCC);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "while parsing `{}` chunk field `{}`", _0, _1)]
pub(crate) struct WhileParsingField<T>(pub(crate) FourCC, pub(crate) T);

impl ReportableError for ParseError {
    #[cfg(feature = "error-detail")]
    type Stack = mediasan_common::error::ReportStack;
    #[cfg(not(feature = "error-detail"))]
    type Stack = mediasan_common::error::NullReportStack;
}

impl<T> ParseResultExt for Result<T, ParseError> {}
