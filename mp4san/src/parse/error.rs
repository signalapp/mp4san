use std::any::type_name;
use std::fmt::{Debug, Display};
use std::io;

use derive_more::Display;

use super::{BoxType, FourCC};

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Invalid box layout")]
    InvalidBoxLayout,
    #[error("Invalid input")]
    InvalidInput,
    #[error("Missing required `{0}` box")]
    MissingRequiredBox(BoxType),
    #[error("Truncated box")]
    TruncatedBox,
    #[error("Unsupported box `{0}`")]
    UnsupportedBox(BoxType),
    #[error("Unsupported box layout")]
    UnsupportedBoxLayout,
    #[error("Unsupported format `{0}`")]
    UnsupportedFormat(FourCC),
}

pub(crate) trait ParseResultExt: error_stack::ResultExt + Sized {
    fn while_parsing_type<T>(self) -> Self {
        self.attach_printable(WhileParsingType(type_name::<T>()))
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

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "multiple `{}` boxes", _0)]
pub(crate) struct MultipleBoxes(pub(crate) BoxType);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "while parsing value of type `{}`", _0)]
pub(crate) struct WhileParsingType<T>(pub(crate) T);

impl WhileParsingType<&'static str> {
    pub fn new<T>() -> Self {
        Self(type_name::<T>())
    }
}

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "while parsing `{}` box", _0)]
pub(crate) struct WhileParsingBox(pub(crate) BoxType);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "while parsing `{}` box field `{}`", _0, _1)]
pub(crate) struct WhileParsingField<T>(pub(crate) BoxType, pub(crate) T);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "while parsing `{}` box child `{}`", _0, _1)]
pub(crate) struct WhileParsingChild(pub(crate) BoxType, pub(crate) BoxType);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "where `{} = {}`", _0, _1)]
pub(crate) struct WhereEq<T, U>(pub(crate) T, pub(crate) U);

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

impl<T: error_stack::ResultExt> ParseResultExt for T {}
