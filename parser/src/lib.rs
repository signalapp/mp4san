use std::collections::TryReserveError;
use std::io;
use std::io::{Read, Write};
use std::mem::size_of;

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

    /// Serialize the box into a writer.
    ///
    /// This will make many small writes—you probably want to use a buffered writer.
    ///
    /// The derived implementation of this method does not allocate. Manual implementors should
    /// ensure that their implementations do not allocate either. However, callers should beware
    /// writers that may allocate, such as [`BufWriter`](std::io::BufWriter) and `Vec<u8>`—if such a
    /// writer is used, and it tries and fails to allocate, it may panic or abort.
    fn write_to<W: Write + ?Sized>(&self, output: &mut W) -> Result<(), Error>;

    /// Deserialize a box from a reader.
    ///
    /// The header should have already been read, and the reader should be positioned immediately
    /// after the size and type fields (and after the largesize and usertype fields, if present).
    /// This can be accomplished by using [`read_header`].
    ///
    /// `size` should be the size of the box, excluding the header. An error will be returned
    /// (potentially after reading some data) if this does not exactly match the amount that
    /// `read_from()` expects to read. If the box's size is _until end-of-file_, it is the caller's
    /// responsibility to find the difference between the current position and the end of the file,
    /// and pass it. Zero means zero: no data will be read, and an error will be returned unless it
    /// valid for this box to be serialized with no fields.
    ///
    /// This will make many small reads—you probably want to use a buffered reader.
    fn read_from<R: Read + ?Sized>(input: &mut R, size: u64) -> Result<Self, Error>
    where
        Self: Sized;

    /// Serialize the box into a new byte vector.
    ///
    /// # Panics
    /// Panics or aborts if the vector can't be allocated.
    fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        self.to_bytes_try_reserve()
            .map_err(|e| e.handle_alloc_error())
    }

    /// Serialize the box into a new byte vector.
    ///
    /// Returns `Err` if the vector can't be allocated.
    fn to_bytes_try_reserve(&self) -> Result<Vec<u8>, AllocOrError> {
        let mut buffer = Vec::new();
        let len = self.size().to_explicit_size().unwrap();
        let len = len.try_into().unwrap_or(usize::MAX);
        buffer.try_reserve(len)?;
        buffer.resize(len, b'\0');
        self.write_to(&mut buffer.as_mut_slice())?;
        Ok(buffer)
    }
}

/// Deserialize the header of an mp4 box from a reader.
///
/// Returns the size and type of the box, which can be used to choose a concrete [`Mp4Box`]
/// implementor with which to deserialize the rest of the box.
///
/// If `Err` is returned, it is unspecified how much has been read.
pub fn read_header<R: Read + ?Sized>(input: &mut R) -> Result<(BoxSize, BoxType), Error> {
    let mut buffer = [0; 4];
    let compact_size = {
        input.read_exact(&mut buffer)?;
        let raw_size = u64::from(u32::from_be_bytes(buffer));
        match BoxSize::new(raw_size) {
            Ok(size) => Some(size),
            Err(BoxSizeError::Extended) => None,
            Err(BoxSizeError::Impossible) => return Err(Error::ImpossibleBoxSize(raw_size, None)),
        }
    };
    input.read_exact(&mut buffer)?;
    let compact_type = if &buffer != b"uuid" {
        Some(u32::from_be_bytes(buffer))
    } else {
        None
    };
    // BoxSize does do a basic minimum size check, enough to read the compact size and type fields,
    // but a given box can have a larger minimum size than that because of the extended fields.
    let min_size = {
        let mut min_size = size_of::<u32>() + size_of::<u32>();
        if compact_size.is_none() {
            min_size += size_of::<u64>();
        }
        if compact_type.is_none() {
            min_size += size_of::<[u8; 16]>();
        }
        min_size as u64
    };
    match compact_size.and_then(|n| n.to_explicit_size()) {
        Some(size) if size < min_size => {
            let type_ = compact_type.map(BoxType::Compact);
            return Err(Error::ImpossibleBoxSize(size, type_));
        }
        _ => {}
    }
    let size = match compact_size {
        Some(size) => size,
        None => {
            let mut buffer = [0; 8];
            input.read_exact(&mut buffer)?;
            let raw_extended_size = u64::from_be_bytes(buffer);
            match BoxSize::new(raw_extended_size) {
                Ok(mut size) if size.to_explicit_size().map_or(false, |n| n >= min_size) => {
                    size.originally_extended = true;
                    size
                }
                _ => {
                    // *record scratch* *freeze frame* Yep, that's me. You're probably wondering how
                    // I got here. Let me tell you, I'm wondering the same thing.
                    let type_ = compact_type.map(BoxType::Compact);
                    return Err(Error::ImpossibleBoxSize(raw_extended_size, type_));
                }
            }
        }
    };
    let type_ = match compact_type {
        Some(compact_type) => BoxType::Compact(compact_type),
        None => {
            let mut buffer = [0; 16];
            input.read_exact(&mut buffer)?;
            BoxType::Extended(Uuid::from_bytes(buffer))
        }
    };
    Ok((size, type_))
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
    /// Returns `(type_, usertype)`, as found in serialized box headers.
    ///
    /// The returned tuple is either `(type_, None)` (compact type) or `(0x75756964, Some(type_))`
    /// (extended type—`0x75756964` being the encoding of `"uuid"`).
    pub fn to_serialized_type(self) -> (u32, Option<[u8; 16]>) {
        match self {
            BoxType::Compact(type_) => (type_, None),
            BoxType::Extended(type_) => (u32::from_be_bytes(*b"uuid"), Some(type_.into_bytes())),
        }
    }

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
/// Not all of the possible box sizes are valid (see [`Error::ImpossibleBoxSize`] for more on this).
/// Constructing a `BoxSize` checks that the size is at least `8` (the minimum possible for all
/// boxes) or exactly `0` (which is taken to mean the box spans until the end of its file).
#[derive(Debug, Copy, Clone)]
pub struct BoxSize {
    inner: u64,
    originally_extended: bool,
}

impl BoxSize {
    /// Creates a `BoxSize` from a size value (as found in serialized box headers).
    ///
    /// See [the type level documentation](Self) for more.
    pub fn new(serialized_size: u64) -> Result<Self, BoxSizeError> {
        let fits_in_u32 = u32::try_from(serialized_size).is_ok();
        match serialized_size {
            0 | 8.. => Ok(Self {
                inner: serialized_size,
                originally_extended: !fits_in_u32,
            }),
            1 => Err(BoxSizeError::Extended),
            2..=7 => Err(BoxSizeError::Impossible),
        }
    }

    /// Returns `(size, largesize)`, as found in serialized box headers.
    ///
    /// The returned tuple is either `(size, None)` (compact size), `(1, Some(size))` (extended
    /// size), or `(0, None)` (size is _until end-of-file_).
    pub fn to_serialized_size(self) -> (u32, Option<u64>) {
        if self.is_until_eof() {
            return (0, None);
        }
        match u32::try_from(self.inner) {
            Ok(size) => (size, None),
            Err(_) => (1, Some(self.inner)),
        }
    }

    /// Returns the explicit size represented by `self`.
    ///
    /// Returns `None` if the size is _until end-of-file_.
    pub fn to_explicit_size(self) -> Option<u64> {
        if self.is_explicit() {
            Some(self.inner)
        } else {
            None
        }
    }

    /// Returns `true` if the size is explicit.
    pub fn is_explicit(self) -> bool {
        !self.is_until_eof()
    }

    /// Returns `true` if the size is _until end-of-file_.
    ///
    /// This is the opposite of [`is_explicit`](Self::is_explicit).
    pub fn is_until_eof(self) -> bool {
        self.inner == 0
    }

    /// Returns `true` if the size is compact **and explicit.**
    ///
    /// This is not quite the opposite of [`is_extended`](Self::is_extended): both will return
    /// `false` for non-explicit sizes.
    pub fn is_compact(self) -> bool {
        self.is_explicit() && !self.is_extended()
    }

    /// Returns `true` if the size is extended **and explicit.**
    ///
    /// This is not quite the opposite of [`is_compact`](Self::is_compact): both will return `false`
    /// for non-explicit sizes.
    pub fn is_extended(self) -> bool {
        self.is_explicit() && self.to_serialized_size().1.is_some()
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

/// This crate's general error type.
#[derive(Debug)]
pub enum Error {
    /// A box was encountered with an impossible size.
    ///
    /// Contains the impossible size value, and the type of the box (if it could be parsed enough
    /// to read the type).
    ///
    /// Every box must have a size of at least `8`, for the size and type fields. If a box has an
    /// extended size, it must be at least `16`; if it has an extended type, it must be at least
    /// `24` (and if it has both, it must be at least `32`).
    ImpossibleBoxSize(u64, Option<BoxType>),

    /// An I/O error occurred.
    Io(io::Error),
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Io(error)
    }
}

/// An error type that can also represent an allocation error.
#[derive(Debug)]
pub enum AllocOrError {
    /// An allocation attempt failed.
    Alloc(TryReserveError),
    /// Another kind of error occurred. See [`Error`].
    Other(Error),
}

impl AllocOrError {
    #[track_caller]
    fn handle_alloc_error(self) -> Error {
        match self {
            AllocOrError::Alloc(alloc_error) => Err(alloc_error).unwrap(),
            AllocOrError::Other(other_error) => other_error,
        }
    }
}

impl From<Error> for AllocOrError {
    fn from(error: Error) -> Self {
        AllocOrError::Other(error)
    }
}

impl From<TryReserveError> for AllocOrError {
    fn from(error: TryReserveError) -> Self {
        AllocOrError::Alloc(error)
    }
}

#[derive(Debug, PartialEq, Mp4Box)]
#[box_type = b"\xffX0\x00"]
pub struct NotARealBox {
    pub bar_ax: u64,
    pub foo_by: u32,
}

#[derive(Debug, PartialEq, Mp4Box)]
#[box_type = 4283969538] // 0xff583002
pub struct AnotherFakeBox;

#[derive(Debug, PartialEq, Mp4Box)]
#[box_type = "c12fdd3f-1e93-464c-baee-7c4480628f58"]
pub struct FakeUuidTypeBox;

#[derive(Debug, PartialEq, Mp4Box)]
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

    use std::io::Cursor;

    use uuid::uuid;

    #[test]
    fn test_size_simple() {
        let not_a_real = NotARealBox {
            bar_ax: u64::MAX,
            foo_by: u32::MAX,
        };
        assert_eq!(not_a_real.size().to_explicit_size(), Some(4 + 4 + 8 + 4));
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

    #[test]
    fn test_ser_simple() {
        let not_a_real = NotARealBox {
            bar_ax: !0 >> 1,
            foo_by: !0 >> 1,
        };
        assert_eq!(
            not_a_real.to_bytes().unwrap(),
            b"\0\0\0\x14\xffX0\0\x7f\xff\xff\xff\xff\xff\xff\xff\x7f\xff\xff\xff"
        );
    }

    #[test]
    fn test_ser_exttype() {
        let fake_box = FakeUuidTypeBox;
        assert_eq!(
            fake_box.to_bytes().unwrap(),
            b"\0\0\0\x18uuid\xc1\x2f\xdd\x3f\x1e\x93\x46\x4c\xba\xee\x7c\x44\x80\x62\x8f\x58"
        );
    }

    #[test]
    fn read_header_simple() {
        let mut data = Cursor::new(b"\0\0\0\x14ftyp");
        let (size, type_) = read_header(&mut data).unwrap();
        assert_eq!(size.to_explicit_size(), Some(20));
        assert_eq!(type_, BoxType::Compact(0x66747970));
    }

    #[test]
    fn read_header_smallest() {
        let mut data = Cursor::new(b"\0\0\0\x08\0\0\0\0");
        let (size, type_) = read_header(&mut data).unwrap();
        assert_eq!(size.to_explicit_size(), Some(8));
        assert_eq!(type_, BoxType::Compact(0));
    }

    #[test]
    fn read_header_extended() {
        let mut data = Cursor::new(
            b"\0\0\0\x18uuid\xc1\x2f\xdd\x3f\x1e\x93\x46\x4c\xba\xee\x7c\x44\x80\x62\x8f\x58",
        );
        let (size, type_) = read_header(&mut data).unwrap();
        assert_eq!(size.to_explicit_size(), Some(24));
        assert_eq!(
            type_,
            BoxType::Extended(uuid!("c12fdd3f-1e93-464c-baee-7c4480628f58"))
        );
    }

    #[test]
    fn read_header_too_small_compact() {
        let mut data = Cursor::new(b"\0\0\0\x07tttt");
        assert!(matches!(
            read_header(&mut data),
            Err(Error::ImpossibleBoxSize(..))
        ));
    }

    #[test]
    fn read_header_too_small_exttype() {
        let mut data = Cursor::new(
            b"\0\0\0\x17uuid\xc1\x2f\xdd\x3f\x1e\x93\x46\x4c\xba\xee\x7c\x44\x80\x62\x8f\x58",
        );
        assert!(matches!(
            read_header(&mut data),
            Err(Error::ImpossibleBoxSize(..))
        ));
    }

    #[test]
    fn read_header_too_small_extsize() {
        let mut data = Cursor::new(b"\0\0\0\x01tttt\0\0\0\0\0\0\0\x0f");
        assert!(matches!(
            read_header(&mut data),
            Err(Error::ImpossibleBoxSize(..))
        ));
    }

    #[test]
    fn read_header_too_small_extsize_exttype() {
        let mut data = Cursor::new(b"\0\0\0\x01uuid\0\0\0\0\0\0\0\x1f\xc1\x2f\xdd\x3f\x1e\x93\x46\x4c\xba\xee\x7c\x44\x80\x62\x8f\x58");
        assert!(matches!(
            read_header(&mut data),
            Err(Error::ImpossibleBoxSize(..))
        ));
    }

    #[test]
    fn read_header_extsize_zero() {
        let mut data = Cursor::new(b"\0\0\0\x01tttt\0\0\0\0\0\0\0\0");
        assert!(matches!(
            read_header(&mut data),
            Err(Error::ImpossibleBoxSize(..))
        ));
    }

    #[test]
    fn read_header_extsize_extsize() {
        let mut data = Cursor::new(b"\0\0\0\x01tttt\0\0\0\0\0\0\0\x01");
        assert!(matches!(
            read_header(&mut data),
            Err(Error::ImpossibleBoxSize(..))
        ));
    }

    #[test]
    fn write() {
        let not_a_real = NotARealBox {
            bar_ax: 0x0102030405060708,
            foo_by: 0x090a0b0c,
        };
        let bytes = b"\0\0\0\x14\xffX0\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c";
        assert_eq!(not_a_real.to_bytes().unwrap(), bytes);
    }

    #[test]
    fn read() {
        let mut data =
            Cursor::new(b"\0\0\0\x14\xffX0\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c");
        let (size, _) = read_header(&mut data).unwrap();
        assert_eq!(size.to_explicit_size(), Some(20));
        assert_eq!(
            NotARealBox::read_from(&mut data, 12).unwrap(),
            NotARealBox {
                bar_ax: 0x0102030405060708,
                foo_by: 0x090a0b0c
            }
        );
    }
}
