use std::io;

use super::{BoxType, FourCC};

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Invalid box layout: {0}")]
    InvalidBoxLayout(&'static str),
    #[error("Invalid input: {0}")]
    InvalidInput(&'static str),
    #[error("Missing required box: {0}")]
    MissingRequiredBox(BoxType),
    #[error("Truncated box")]
    TruncatedBox,
    #[error("Unsupported box: {0}")]
    UnsupportedBox(BoxType),
    #[error("Unsupported box layout: {0}")]
    UnsupportedBoxLayout(&'static str),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(FourCC),
}

impl From<ParseError> for io::Error {
    fn from(from: ParseError) -> Self {
        use ParseError::*;
        match from {
            err @ (InvalidBoxLayout { .. }
            | UnsupportedBox { .. }
            | UnsupportedBoxLayout { .. }
            | MissingRequiredBox { .. }
            | UnsupportedFormat { .. }
            | TruncatedBox { .. }) => io::Error::new(io::ErrorKind::InvalidData, err),
            err @ InvalidInput { .. } => io::Error::new(io::ErrorKind::InvalidInput, err),
        }
    }
}
