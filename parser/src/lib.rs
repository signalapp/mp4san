// Used by the derive macros' generated code.
use crate as mp4san_isomparse;

pub use mp4san_isomparse_macros::Mp4Box;
pub use uuid::Uuid;

/// An object (box, atom) in the mp4 file structure.
///
/// A box is defined by its [type identifier](BoxType) and its [size](BoxSize).
pub trait Mp4Box {
    /// Returns the size (length) of the box.
    fn size(&self) -> BoxSize;

    /// Returns the type identifier of the box.
    ///
    /// Since each box type is modeled as a separate type in Rust, this could have been an
    /// associated function (with no `self`) or even an associated const. It is modeled as a method,
    /// however, because either of those alternatives would make the trait non-object-safe.
    fn type_(&self) -> BoxType;
}

/// The type code of an mp4 box.
///
/// Every box has a type. Boxes defined by ISO standard have a _compact_, u32 type; other boxes have
/// an _extended_, UUID type.
///
/// Extended types of the form `XXXXXXXX-0011-0010-8000-00aa00389b71` are reserved by ISO to
/// represent compact types (the first 32 bits, shown as `XXXXXXXX`, hold the compact type code
/// being represented). **These extended types should not be used:** files containing them are not
/// compliant with the specification, implementations are explicitly not required to recognize
/// them, and this implementation in particular will treat them as unknown extended types.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BoxType {
    Compact(u32),
    Extended(Uuid),
}

impl BoxType {
    /// Returns `true` if the type is a compact/standard type.
    pub fn is_compact(self) -> bool {
        !self.is_extended()
    }

    /// Returns `true` if the type is an extended/private type.
    ///
    /// This is the opposite of [`is_compact`](Self::is_compact).
    pub fn is_extended(self) -> bool {
        matches!(self, BoxType::Extended(_))
    }
}

/// The size of an mp4 box (including the size and type fields).
///
/// Not all of the possible box sizes are valid. In particular, because boxes always start with
/// a u32 (compact) size and a u32 (compact) type field, sizes less than `8` are not valid.
/// Constructing a `BoxSize` checks that the size is at least `8`, or else `0` (meaning that the box
/// extends until the end of the file).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct BoxSize(u64);

impl BoxSize {
    /// Creates a `BoxSize` from a size value (as found in serialized box headers).
    ///
    /// See [the type level documentation](Self) for more.
    pub fn new(serialized_size: u64) -> Result<Self, BoxSizeError> {
        match serialized_size {
            0 | 8.. => Ok(Self(serialized_size)),
            1 => Err(BoxSizeError::Extended),
            2..=7 => Err(BoxSizeError::Impossible),
        }
    }

    /// Returns the explicit size represented by `self`.
    ///
    /// Returns `Some` if `self` is an explicit size, or `None` if it indicates that the relevant
    /// box extends until the end of the file.
    pub fn to_explicit_size(self) -> Option<u64> {
        if self.is_explicit() {
            Some(self.0)
        } else {
            None
        }
    }

    /// Returns `true` if the size is explicit.
    pub fn is_explicit(self) -> bool {
        !self.is_until_eof()
    }

    /// Returns `true` if the size is _extends until end-of-file_.
    ///
    /// This is the opposite of [`is_explicit`](Self::is_explicit).
    pub fn is_until_eof(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if the size is compact **and explicit.**
    ///
    /// This is not quite the opposite of [`is_extended`](Self::is_extended): both will return
    /// `false` for non-explicit sizes.
    pub fn is_compact(self) -> bool {
        self.is_explicit() && u32::try_from(self.0).is_ok()
    }

    /// Returns `true` if the size is extended **and explicit.**
    ///
    /// This is not quite the opposite of [`is_compact`](Self::is_compact): both will return `false`
    /// for non-explicit sizes.
    pub fn is_extended(self) -> bool {
        self.is_explicit() && !self.is_compact()
    }
}

/// The error type for interpreting serialized box sizes.
#[derive(Debug)]
pub enum BoxSizeError {
    /// Returned by `BoxSize` constructors if the size value is in `2..8` (these are impossible).
    Impossible,

    /// Returned by `BoxSize` constructors if the size value is `1`.
    ///
    /// This indicates that the box's size is given in the extended “largesize” field, and so that
    /// field should be read.
    Extended,
}

#[derive(Mp4Box)]
#[box_type = b"\xffX0\x00"]
pub struct NotARealBox {
    pub bar_ax: u64,
    pub foo_by: u32,
}

#[derive(Mp4Box)]
#[box_type = 0xff583001]
pub enum FakeEnumBox {
    A { foo: u32 },
    B(u64),
    C,
}

#[derive(Mp4Box)]
#[box_type = 4283969538] // 0xff583002
pub struct AnotherFakeBox;

#[derive(Mp4Box)]
#[box_type = "c12fdd3f-1e93-464c-baee-7c4480628f58"]
pub struct FakeUuidTypeBox;

#[derive(Mp4Box)]
#[box_type = "xa04"]
pub struct Fifth;

//impl Mp4Box for NotARealBox {
//    fn size(&self) -> u64 {
//        0 + size_of::<u64>() + size_of::<u32>()
//    }
//    fn type_(&self) -> BoxType {
//        BoxType::Compact(/* whatever the #[box_type] says */)
//    }
//}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_simple() {
        let not_a_real = NotARealBox {
            bar_ax: u64::MAX,
            foo_by: u32::MAX,
        };
        assert_eq!(not_a_real.size().to_explicit_size(), Some(4 + 4 + 8 + 4));
    }

    #[test]
    fn test_size_enum_a() {
        let fake_enum = FakeEnumBox::A { foo: u32::MAX };
        assert_eq!(fake_enum.size().to_explicit_size(), Some(4 + 4 + 4));
    }

    #[test]
    fn test_size_enum_b() {
        let fake_enum = FakeEnumBox::B(u64::MAX);
        assert_eq!(fake_enum.size().to_explicit_size(), Some(4 + 4 + 8));
    }

    #[test]
    fn test_size_enum_c() {
        let fake_enum = FakeEnumBox::C;
        assert_eq!(fake_enum.size().to_explicit_size(), Some(4 + 4));
    }

    #[test]
    fn test_size_exttype() {
        let fake_box = FakeUuidTypeBox;
        assert_eq!(fake_box.size().to_explicit_size(), Some(4 + 4 + 16));
    }

    #[test]
    fn test_type_bytes() {
        let not_a_real = NotARealBox {
            bar_ax: u64::MAX,
            foo_by: u32::MAX,
        };
        assert_eq!(not_a_real.type_(), BoxType::Compact(0xff583000));
    }

    #[test]
    fn test_type_compact_int_hex() {
        let fake_enum = FakeEnumBox::A { foo: u32::MAX };
        assert_eq!(fake_enum.type_(), BoxType::Compact(0xff583001));
    }

    #[test]
    fn test_type_compact_int_decimal() {
        let fake_box = AnotherFakeBox;
        assert_eq!(fake_box.type_(), BoxType::Compact(0xff583002));
    }

    #[test]
    fn test_type_extended() {
        let fake_box = FakeUuidTypeBox;
        let expected = BoxType::Extended(Uuid::from_u128(0xc12fdd3f_1e93_464c_baee_7c4480628f58));
        assert_eq!(fake_box.type_(), expected);
    }

    #[test]
    fn test_type_compact_str() {
        let fake_box = Fifth;
        assert_eq!(fake_box.type_(), BoxType::Compact(0x78613034));
    }
}
