//! Unstable API used by the `#[derive(ParseBoxes)]` macro's generated code.

use std::iter;
use std::option;
use std::vec;

use derive_where::derive_where;
use nonempty::NonEmpty;

use crate::parse::ParseBox;

/// A type which can be used as a field in a `#[derive(ParseBoxes)]`.
pub trait Field {
    /// The type of box that this field contains.
    ///
    /// For example, for `Vec<T>`, this would be `T`.
    type Type: ParseBox;

    /// A type which can accumulate values of type `T` to construct a container like `Self`.
    ///
    /// For example, for `Vec<U>`, this could be `Vec<T>`. For a plain `U`, this could be `Option<U>`.
    type Accumulator<T>: Accumulator<T>;

    /// The type returned by `<Self as Field>::into_iter`.
    type IntoIter: Iterator<Item = Self::Type>;

    /// Creates an [`Iterator`] of boxes contained in this field.
    fn into_iter(self) -> Self::IntoIter;
}

/// A type which can accumulate values and unwrap into another type containing those values.
pub trait Accumulator<T> {
    /// The type that this accumulator [unwraps](Self::unwrap) to.
    type Unwrapped;

    /// Appends an element to the accumulator.
    fn push(&mut self, field: T);

    /// Returns `true` if no more elements can be accumulated.
    fn is_full(&self) -> bool;

    /// Constructs a container from the values accumulated, if possible.
    fn unwrap(self) -> Option<Self::Unwrapped>;
}

#[doc(hidden)]
#[derive(Clone)]
#[derive_where(Default)]
/// An [`Accumulator`] to build a [`T`].
pub struct PlainFieldAccumulator<T> {
    value: Option<T>,
}

#[doc(hidden)]
#[derive(Clone)]
#[derive_where(Default)]
/// An [`Accumulator`] of `T`s to build a [`NonEmpty<T>`].
pub struct NonEmptyAccumulator<T> {
    head: Option<T>,
    tail: Vec<T>,
}

//
// Field impls
//

impl<T: ParseBox> Field for T {
    type Type = Self;
    type Accumulator<U> = PlainFieldAccumulator<U>;
    type IntoIter = iter::Once<T>;

    fn into_iter(self) -> Self::IntoIter {
        iter::once(self)
    }
}

impl<T: ParseBox> Field for Option<T> {
    type Type = T;
    type Accumulator<U> = Option<U>;
    type IntoIter = option::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(self)
    }
}

impl<T: ParseBox> Field for Vec<T> {
    type Type = T;
    type Accumulator<U> = Vec<U>;
    type IntoIter = vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(self)
    }
}

impl<T: ParseBox> Field for NonEmpty<T> {
    type Type = T;
    type Accumulator<U> = NonEmptyAccumulator<U>;
    type IntoIter = <NonEmpty<T> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(self)
    }
}

//
// Accumulator impls
//

impl<T> Accumulator<T> for Option<T> {
    type Unwrapped = Option<T>;

    fn push(&mut self, field: T) {
        *self = Some(field);
    }

    fn is_full(&self) -> bool {
        self.is_some()
    }

    fn unwrap(self) -> Option<Self::Unwrapped> {
        Some(self)
    }
}

impl<T> Accumulator<T> for Vec<T> {
    type Unwrapped = Self;

    fn push(&mut self, field: T) {
        self.push(field);
    }

    fn is_full(&self) -> bool {
        false
    }

    fn unwrap(self) -> Option<Self::Unwrapped> {
        Some(self)
    }
}

//
// PlainFieldAccumulator impls
//

impl<T> Accumulator<T> for PlainFieldAccumulator<T> {
    type Unwrapped = T;

    fn push(&mut self, field: T) {
        self.value = Some(field);
    }

    fn is_full(&self) -> bool {
        self.value.is_some()
    }

    fn unwrap(self) -> Option<Self::Unwrapped> {
        self.value
    }
}

//
// NonEmptyAccumulator impls
//

impl<T> Accumulator<T> for NonEmptyAccumulator<T> {
    type Unwrapped = NonEmpty<T>;

    fn push(&mut self, field: T) {
        if self.head.is_none() {
            self.head = Some(field);
        } else {
            self.tail.push(field);
        }
    }

    fn is_full(&self) -> bool {
        false
    }

    fn unwrap(self) -> Option<Self::Unwrapped> {
        self.head.map(|head| NonEmpty { head, tail: self.tail })
    }
}
