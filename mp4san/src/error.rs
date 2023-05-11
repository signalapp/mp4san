//! Error types returned by the public API.

use std::io;

use error_stack::Report;

use crate::parse::ParseError;

/// Error type returned by `mp4san`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An IO error occurred while reading the given input.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// The input could not be parsed as an MP4 file.
    ///
    /// This type of error contains an [`error_stack::Report`] which can be used to identify exactly where in the parser
    /// the error occurred. The [`Display`](std::fmt::Display) implementation, for example, will print a human-readable
    /// parser stack trace. The underlying [`ParseError`] cause can also be retrieved e.g. for matching against with
    /// [`Report::current_context`].
    #[error("Parse error: {0}")]
    Parse(Report<ParseError>),
}

//
// Error impls
//

impl From<Report<ParseError>> for Error {
    fn from(from: Report<ParseError>) -> Self {
        Self::Parse(from)
    }
}
