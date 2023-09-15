//! Error types returned by the public API.

use crate::parse::ParseError;

//
// public types
//

/// Error type returned by `mp4san`.
pub type Error = mediasan_common::error::Error<ParseError>;

pub use mediasan_common::Report;

//
// private types
//

pub(crate) type Result<T, E> = std::result::Result<T, Report<E>>;

#[doc(hidden)]
pub use mediasan_common::ResultExt as __ResultExt;

pub(crate) use self::__ResultExt as ResultExt;
