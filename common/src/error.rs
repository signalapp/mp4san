//! Error types returned by the public API.

use std::any::type_name;
use std::fmt;
use std::fmt::{Debug, Display};
use std::io;
use std::panic::Location;
use std::result::Result as StdResult;

use derive_more::Display;

//
// public types
//

/// Error type returned by `mediasan`.
#[derive(Debug, thiserror::Error)]
pub enum Error<E: ReportableError> {
    /// An IO error occurred while reading the given input.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// The input could not be parsed as a media file.
    #[error("Parse error: {0}")]
    Parse(#[from] Report<E>),
}

/// A report with additional debugging info for an error.
///
/// A `Report<E>` can be used to identify exactly where the error `E` occurred in `mediasan`. The [`Debug`]
/// implementation will print a human-readable parser stack trace. The underlying error of type `E` can also be
/// retrieved e.g. for matching against with [`get_ref`](Self::get_ref) or [`into_inner`](Self::into_inner).
#[derive(thiserror::Error)]
#[error("{error}")]
pub struct Report<E: ReportableError> {
    #[source]
    error: E,
    stack: E::Stack,
}

/// A [`Display`]-able indicating there was extra trailing input after parsing.
#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "extra unparsed input")]
pub struct ExtraUnparsedInput;

/// A [`Display`]-able indicating an error occurred while parsing a certain type.
#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "while parsing value of type `{}`", _0)]
pub struct WhileParsingType(&'static str);

/// A convenience type alias for a [`Result`](std::result::Result) containing an error wrapped by a [`Report`].
pub type Result<T, E> = StdResult<T, Report<E>>;

/// An trait providing [`Report`]-related extensions for [`Result`](std::result::Result).
pub trait ResultExt: Sized {
    #[track_caller]
    /// Attach a [`Display`]-able type to the error [`Report`]'s stack trace.
    fn attach_printable<P: Display + Send + Sync + 'static>(self, printable: P) -> Self;

    #[track_caller]
    /// Attach the message "while parsing type T" to the error [`Report`]'s stack trace.
    fn while_parsing_type(self) -> Self;
}

/// An error stack.
pub struct ReportStack {
    location: &'static Location<'static>,
    entries: Vec<ReportEntry>,
}

/// A null error stack which ignores all data attached to it.
#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "")]
pub struct NullReportStack;

/// A trait for error types which can be used in a [`Report`].
pub trait ReportableError: Display {
    /// The error stack type corresponding to this error.
    type Stack: ReportableErrorStack;
}

/// A trait for error stack types for use within a [`Report`].
pub trait ReportableErrorStack: Display {
    #[track_caller]
    /// Construct a new instance of [`Self`].
    fn new() -> Self;

    #[track_caller]
    /// Attach a [`Display`]-able type to the error [`Report`]'s stack trace.
    fn attach_printable<P: Display + Send + Sync + 'static>(self, printable: P) -> Self;
}

//
// private types
//

#[derive(derive_more::Display)]
#[display(fmt = "{message} at {location}")]
struct ReportEntry {
    message: Box<dyn Display + Send + Sync + 'static>,
    location: &'static Location<'static>,
}

//
// Report impls
//

impl<E: ReportableError> Report<E> {
    /// Get a reference to the underlying error.
    pub fn get_ref(&self) -> &E {
        &self.error
    }

    /// Unwrap this report, returning the underlying error.
    pub fn into_inner(self) -> E {
        self.error
    }

    #[track_caller]
    /// Attach a [`Display`]-able type to the stack trace.
    pub fn attach_printable<P: Display + Send + Sync + 'static>(mut self, message: P) -> Self {
        self.stack = self.stack.attach_printable(message);
        self
    }
}

impl<E: ReportableError> From<E> for Report<E> {
    #[track_caller]
    fn from(error: E) -> Self {
        Self { error, stack: E::Stack::new() }
    }
}

impl<E: ReportableError> Debug for Report<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { error, stack } = self;
        write!(f, "{error}{stack}")
    }
}

//
// ReportErrorStack impls
//

//
// WhileParsingType impls
//

impl WhileParsingType {
    /// Construct a new [`WhileParsingType`] where the type described is `T`.
    pub fn new<T: ?Sized>() -> Self {
        Self(type_name::<T>())
    }
}

//
// ReportStack impls
//

impl Display for ReportStack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { location, entries } = self;
        writeln!(f, " at {location}")?;
        for entry in &entries[..self.entries.len().saturating_sub(1)] {
            writeln!(f, " - {entry}")?;
        }
        if let Some(entry) = entries.last() {
            write!(f, " - {entry}")?;
        }
        Ok(())
    }
}

impl ReportableErrorStack for ReportStack {
    #[track_caller]
    fn new() -> Self {
        Self { location: Location::caller(), entries: Default::default() }
    }

    fn attach_printable<P: Display + Send + Sync + 'static>(mut self, printable: P) -> Self {
        let entry = ReportEntry { message: Box::new(printable), location: Location::caller() };
        self.entries.push(entry);
        self
    }
}

//
// ReportableErrorStack impls
//

impl ReportableErrorStack for NullReportStack {
    fn new() -> Self {
        Self
    }

    fn attach_printable<P: Display + Send + Sync + 'static>(self, _printable: P) -> Self {
        Self
    }
}

//
// ResultExt impls
//

impl<T, E: ReportableError> ResultExt for Result<T, E> {
    #[track_caller]
    fn attach_printable<P: Display + Send + Sync + 'static>(self, printable: P) -> Self {
        match self {
            Ok(value) => Ok(value),
            Err(err) => Err(err.attach_printable(printable)),
        }
    }

    #[track_caller]
    fn while_parsing_type(self) -> Self {
        self.attach_printable(WhileParsingType::new::<T>())
    }
}

impl<T, E: ReportableError> ResultExt for StdResult<T, Error<E>> {
    #[track_caller]
    fn attach_printable<P: Display + Send + Sync + 'static>(self, printable: P) -> Self {
        match self {
            Err(Error::Io(err)) => Err(Error::Io(err)),
            Err(Error::Parse(err)) => Err(Error::Parse(err.attach_printable(printable))),
            _ => self,
        }
    }

    #[track_caller]
    fn while_parsing_type(self) -> Self {
        self.attach_printable(WhileParsingType::new::<T>())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const TEST_ERROR_DISPLAY: &str = "test error display";
    const TEST_ATTACHMENT: &str = "test attachment";

    #[derive(Debug, thiserror::Error)]
    #[error("{}", TEST_ERROR_DISPLAY)]
    struct TestError;

    impl ReportableError for TestError {
        type Stack = ReportStack;
    }

    fn test_report() -> Report<TestError> {
        report_attach!(TestError, TEST_ATTACHMENT)
    }

    #[test]
    fn test_report_display() {
        assert_eq!(test_report().to_string(), TEST_ERROR_DISPLAY);
    }

    #[test]
    fn test_report_debug() {
        let report_debug = format!("{report:?}", report = test_report());
        assert!(report_debug.starts_with(TEST_ERROR_DISPLAY));
        assert!(report_debug.contains(TEST_ATTACHMENT));
    }
}
