//! Error types returned by the unstable parsing API.

use std::fmt::{Debug, Display};

use derive_more::Display;
use mediasan_common::error::{ReportStack, ReportableError};

use crate::error::{Result, ResultExt};

use super::{BoxType, FourCC};

/// Error type returned by the MP4 parser.
///
/// While the API of this error type is currently considered unstable, it is more stably guaranteed to implement
/// [`Display`] + [`Debug`].
#[allow(missing_docs)]
#[derive(Clone, Debug, thiserror::Error)]
pub enum ParseError {
    /// The input is invalid because its boxes are in a ordering or configuration disallowed by the ISO specification.
    #[error("Invalid box layout")]
    InvalidBoxLayout,

    /// The input is invalid.
    #[error("Invalid input")]
    InvalidInput,

    /// The input is invalid because it is missing a box required by the ISO specification.
    #[error("Missing required `{_0}` box")]
    MissingRequiredBox(BoxType),

    /// The input is invalid because the input ended before the end of a box.
    ///
    /// This can occur either when the entire input is truncated or when a box size is incorrect.
    #[error("Truncated box")]
    TruncatedBox,

    /// The input is unsupported because it contains an unknown box.
    #[error("Unsupported box `{_0}`")]
    UnsupportedBox(BoxType),

    /// The input is unsupported because its boxes are in an unsupported ordering.
    #[error("Unsupported box layout")]
    UnsupportedBoxLayout,

    /// The input is unsupported because it doesn't contain [`COMPATIBLE_BRAND`](crate::COMPATIBLE_BRAND) in its file
    /// type header (`ftyp`).
    #[error("Unsupported format `{_0}`")]
    UnsupportedFormat(FourCC),
}

#[doc(hidden)]
/// Used by the derive macros' generated code.
pub trait __ParseResultExt: ResultExt + Sized {
    fn while_parsing_box(self, box_type: BoxType) -> Self {
        self.attach_printable(WhileParsingBox(box_type))
    }

    fn while_parsing_field<T>(self, box_type: BoxType, field_name: T) -> Self
    where
        T: Display + Debug + Send + Sync + 'static,
    {
        self.attach_printable(WhileParsingField(box_type, field_name))
    }

    fn while_parsing_child(self, box_type: BoxType, child_box_type: BoxType) -> Self {
        self.attach_printable(WhileParsingChild(box_type, child_box_type))
    }

    fn where_eq<T, U>(self, lhs: T, rhs: U) -> Self
    where
        T: Display + Debug + Send + Sync + 'static,
        U: Display + Debug + Send + Sync + 'static,
    {
        self.attach_printable(WhereEq(lhs, rhs))
    }
}
pub(crate) use self::__ParseResultExt as ParseResultExt;

#[derive(Clone, Copy, Debug, Display)]
#[display("multiple `{}` boxes", _0)]
pub(crate) struct MultipleBoxes(pub(crate) BoxType);

#[derive(Clone, Copy, Debug, Display)]
#[display("while parsing `{}` box", _0)]
pub(crate) struct WhileParsingBox(pub(crate) BoxType);

#[derive(Clone, Copy, Debug, Display)]
#[display("while parsing `{}` box field `{}`", _0, _1)]
pub(crate) struct WhileParsingField<T>(pub(crate) BoxType, pub(crate) T);

#[derive(Clone, Copy, Debug, Display)]
#[display("while parsing `{}` box child `{}`", _0, _1)]
pub(crate) struct WhileParsingChild(pub(crate) BoxType, pub(crate) BoxType);

#[derive(Clone, Copy, Debug, Display)]
#[display("where `{} = {}`", _0, _1)]
pub(crate) struct WhereEq<T, U>(pub(crate) T, pub(crate) U);

impl ReportableError for ParseError {
    type Stack = ReportStack;
}

impl<T> ParseResultExt for Result<T, ParseError> {}
